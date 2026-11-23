use std::fs;
use std::os::unix::fs::PermissionsExt;

use tokio::net::UnixStream;

use super::*;
use crate::{PlaybackStatus, RepeatMode, Status, Track};

#[tokio::test]
async fn unix_socket_round_trips_status_and_closes_bad_requests() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("ipc");
    let path = directory.join("kaset.sock");
    let listener = local::Server::bind(&path).unwrap();
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(local::Server::bind(&path).is_err());

    let expected = Response::Status {
        status: Status {
            track: Some(Track {
                title: "Test song".into(),
                artists: vec!["Test artist".into()],
                video_id: "abc-DEF_123".into(),
            }),
            position: 12.0,
            duration: 180.0,
            status: PlaybackStatus::Playing,
            repeat: RepeatMode::Off,
            shuffle: true,
        },
    };
    let server = tokio::spawn(serve_from(listener, {
        let expected = expected.clone();
        move |command| {
            assert_eq!(command, Command::Status {});
            std::future::ready(expected.clone())
        }
    }));
    let stream = local::connect(&path).await.unwrap();
    assert_eq!(
        exchange(stream, &Command::Status {}).await.unwrap(),
        expected
    );
    for (request, error) in [
        (
            b"{\"v\":1,\"cmd\":\"unknown\"}\n".to_vec(),
            Error::InvalidRequest,
        ),
        (vec![b'x'; MAX_REQUEST_BYTES + 1], Error::RequestTooLarge),
    ] {
        let mut stream = UnixStream::connect(&path).await.unwrap();
        stream.write_all(&request).await.unwrap();
        let mut reply = Vec::new();
        timeout(IO_TIMEOUT, stream.read_to_end(&mut reply))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reply.last(), Some(&b'\n'));
        assert_eq!(
            serde_json::from_slice::<Response>(&reply).unwrap(),
            Response::Error { error }
        );
    }
    server.abort();
    let _ = server.await;
    assert!(matches!(
        local::connect(&path).await,
        Err(ClientError::NotRunning)
    ));
}

#[tokio::test]
async fn unix_server_removes_only_stale_sockets() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().canonicalize().unwrap().join("ipc");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.join("kaset.sock");
    fs::write(&path, "leave me alone").unwrap();
    assert!(local::Server::bind(&path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "leave me alone");
    fs::remove_file(&path).unwrap();
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(matches!(
        local::connect(&path).await,
        Err(ClientError::NotRunning)
    ));
    assert!(local::Server::bind(&path).is_ok());
}
