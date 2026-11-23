use super::*;
use serde_json::json;

#[test]
fn protocol_command_round_trips() {
    for (command, fields) in [
        (Command::Status {}, json!({"cmd":"status"})),
        (Command::PlayPause {}, json!({"cmd":"play_pause"})),
        (Command::Play {}, json!({"cmd":"play"})),
        (Command::Pause {}, json!({"cmd":"pause"})),
        (Command::Next {}, json!({"cmd":"next"})),
        (Command::Previous {}, json!({"cmd":"previous"})),
        (
            Command::Seek { seconds: 12.5 },
            json!({"cmd":"seek","seconds":12.5}),
        ),
        (Command::Queue {}, json!({"cmd":"queue"})),
        (
            Command::PlayVideo {
                video_id: "abc-DEF_123".into(),
            },
            json!({"cmd":"play_video","video_id":"abc-DEF_123"}),
        ),
        (
            Command::PlayPage {
                browse_id: "VL-test".into(),
            },
            json!({"cmd":"play_page","browse_id":"VL-test"}),
        ),
        (
            Command::Shuffle { on: true },
            json!({"cmd":"shuffle","on":true}),
        ),
        (
            Command::Repeat {
                mode: RepeatMode::One,
            },
            json!({"cmd":"repeat","mode":"one"}),
        ),
    ] {
        let encoded = encode_request(&command).unwrap();
        assert_eq!(encoded.last(), Some(&b'\n'));
        let mut expected = fields;
        expected["v"] = json!(1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&encoded).unwrap(),
            expected
        );
        assert_eq!(
            decode_request(&encoded[..encoded.len() - 1]).unwrap(),
            command
        );
    }
}

#[test]
fn protocol_rejects_unknown_versions_commands_and_invalid_arguments() {
    for request in [
        r#"{"v":2,"cmd":"status"}"#,
        r#"{"cmd":"status"}"#,
        r#"{"v":1,"v":1,"cmd":"status"}"#,
        r#"{"v":1,"cmd":"run_js","script":"example"}"#,
        r#"{"v":1,"cmd":"status","url":"https://example.com"}"#,
        r#"{"v":1,"cmd":"play_video","video_id":"abc-DEF_123","url":"https://example.com"}"#,
        r#"{"v":1,"cmd":"play_video","video_id":"a/b-DEF_123"}"#,
        r#"{"v":1,"cmd":"play_video","video_id":"abcdefghij"}"#,
        r#"{"v":1,"cmd":"play_page","browse_id":""}"#,
        r#"{"v":1,"cmd":"play_page","browse_id":"https://example.com"}"#,
        r#"{"v":1,"cmd":"seek","seconds":-1}"#,
        r#"{"v":1,"cmd":"seek","seconds":1e400}"#,
        r#"{"v":1,"cmd":"repeat","mode":"forever"}"#,
    ] {
        assert!(decode_request(request.as_bytes()).is_err(), "{request}");
    }
    assert!(encode_request(&Command::Seek { seconds: f64::NAN }).is_err());
    assert!(
        encode_request(&Command::PlayPage {
            browse_id: "x".repeat(257)
        })
        .is_err()
    );

    let mut line = br#"{"v":1,"cmd":"status"}"#.to_vec();
    line.resize(MAX_REQUEST_BYTES, b' ');
    assert_eq!(decode_request(&line).unwrap(), Command::Status {});
    line.push(b' ');
    assert!(decode_request(&line).is_err());
}

#[test]
fn protocol_round_trips_idle_status() {
    let idle = Response::Status {
        status: Status {
            track: None,
            position: 0.0,
            duration: 0.0,
            status: PlaybackStatus::Idle,
            repeat: RepeatMode::Off,
            shuffle: false,
        },
    };
    let bytes = serde_json::to_vec(&idle).unwrap();
    assert_eq!(serde_json::from_slice::<Response>(&bytes).unwrap(), idle);
}

#[test]
fn task13_browse_id_boundaries_are_shared_by_the_protocol() {
    for id in ["x", &"x".repeat(256)] {
        let command = Command::PlayPage {
            browse_id: id.into(),
        };
        let line = encode_request(&command).unwrap();
        assert_eq!(decode_request(&line[..line.len() - 1]).unwrap(), command);
    }
    for id in ["", "foo/bar", "가", &"x".repeat(257)] {
        assert!(!valid_browse_id(id));
    }
}
