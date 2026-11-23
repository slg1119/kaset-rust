use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Semaphore;
use tokio::time::{sleep, timeout};

use crate::{Command, Error, MAX_REQUEST_BYTES, Response, decode_request, encode_request};

#[cfg(unix)]
#[path = "transport/unix.rs"]
mod local;
#[cfg(windows)]
#[path = "transport/windows.rs"]
mod local;

const MAX_CLIENTS: usize = 16;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(3);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(60);
// A cold page request may wait for launch, key discovery and the browse call.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(70);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Kaset is not running.")]
    NotRunning,
    #[error("Playback IPC failed: {0}")]
    Transport(#[from] io::Error),
    #[error(transparent)]
    Rejected(#[from] Error),
}

pub async fn request(command: Command) -> Result<Response, ClientError> {
    let endpoint = local::endpoint()?;
    match timeout(REQUEST_TIMEOUT, async {
        let stream = local::connect(&endpoint).await?;
        exchange(stream, &command).await
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "Kaset did not reply").into()),
    }
}

pub async fn serve<F, Fut>(dispatch: F) -> io::Result<()>
where
    F: Fn(Command) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    serve_from(local::Server::bind(&local::endpoint()?)?, dispatch).await
}

trait Listener {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    fn accept(&mut self) -> impl Future<Output = io::Result<Self::Stream>> + Send;
}

async fn serve_from<F, Fut>(mut listener: impl Listener, dispatch: F) -> io::Result<()>
where
    F: Fn(Command) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    let dispatch = Arc::new(dispatch);
    let permits = Arc::new(Semaphore::new(MAX_CLIENTS));
    loop {
        let permit = Arc::clone(&permits)
            .acquire_owned()
            .await
            .expect("listener semaphore is never closed");
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(kind = ?error.kind(), os_code = error.raw_os_error(), "local playback IPC accept failed");
                drop(permit);
                // Resource exhaustion and aborted connections must not spin the listener.
                sleep(ACCEPT_RETRY_DELAY).await;
                continue;
            }
        };
        let dispatch = Arc::clone(&dispatch);
        tokio::spawn(async move {
            let _permit = permit;
            // Individual client I/O failures never stop the listener.
            let _ = handle(stream, |command| dispatch(command)).await;
        });
    }
}

async fn read_line(stream: impl AsyncRead + Unpin, limit: usize) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    BufReader::new(stream)
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut line)
        .await?;
    Ok(line)
}

async fn handle<S, F>(mut stream: S, dispatch: impl FnOnce(Command) -> F) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    F: Future<Output = Response>,
{
    let request = match timeout(IO_TIMEOUT, read_line(&mut stream, MAX_REQUEST_BYTES)).await {
        Ok(Ok(mut line)) if line.last() == Some(&b'\n') => {
            line.pop();
            decode_request(&line)
        }
        Ok(Ok(line)) if line.len() > MAX_REQUEST_BYTES => Err(Error::RequestTooLarge),
        Ok(Ok(_)) | Err(_) => Err(Error::InvalidRequest),
        Ok(Err(error)) => return Err(error),
    };
    let response = match request {
        Ok(command) => timeout(DISPATCH_TIMEOUT, async { dispatch(command).await })
            .await
            .unwrap_or(Response::Error {
                error: Error::PlaybackFailed,
            }),
        Err(error) => Response::Error { error },
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    timeout(IO_TIMEOUT, stream.write_all(&bytes)).await??;
    Ok(())
}

async fn exchange<S>(mut stream: S, command: &Command) -> Result<Response, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(&encode_request(command)?).await?;
    let bytes = read_line(&mut stream, MAX_RESPONSE_BYTES).await?;
    if bytes.last() != Some(&b'\n') {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "invalid Kaset response line").into(),
        );
    }
    match serde_json::from_slice(&bytes).map_err(io::Error::from)? {
        Response::Error { error } => Err(error.into()),
        response => Ok(response),
    }
}

#[cfg(all(test, unix))]
mod tests;

#[cfg(test)]
mod regressions {
    use super::*;
    use tokio::io::{DuplexStream, duplex};

    struct FakeListener {
        attempts: usize,
        next: Option<DuplexStream>,
    }

    impl Listener for FakeListener {
        type Stream = DuplexStream;

        async fn accept(&mut self) -> io::Result<Self::Stream> {
            self.attempts += 1;
            match self.attempts {
                1 => Err(io::ErrorKind::ConnectionAborted.into()),
                2 => Ok(self.next.take().unwrap()),
                _ => std::future::pending().await,
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn ipc_review_accept_error_does_not_stop_serving() {
        let (client, stream) = duplex(128);
        let listener = FakeListener {
            attempts: 0,
            next: Some(stream),
        };
        let server = tokio::spawn(serve_from(listener, |command| async move {
            assert_eq!(command, Command::Play {});
            Response::Ok
        }));
        let reply = timeout(Duration::from_secs(1), exchange(client, &Command::Play {})).await;
        server.abort();
        assert!(matches!(reply, Ok(Ok(Response::Ok))), "{reply:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn ipc_review_dispatch_times_out_and_drops_late_work() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let (client, stream) = duplex(128);
        let replaced = Arc::new(AtomicBool::new(false));
        let server = tokio::spawn({
            let replaced = Arc::clone(&replaced);
            handle(stream, move |_| async move {
                tokio::time::sleep(Duration::from_secs(61)).await;
                replaced.store(true, Ordering::Relaxed);
                Response::Ok
            })
        });
        let start = tokio::time::Instant::now();
        let reply = exchange(
            client,
            &Command::PlayPage {
                browse_id: "VL-test".into(),
            },
        )
        .await;
        let elapsed = start.elapsed();
        server.await.unwrap().unwrap();
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(
            (elapsed.as_secs(), replaced.load(Ordering::Relaxed)),
            (60, false)
        );
        assert!(
            matches!(reply, Err(ClientError::Rejected(Error::PlaybackFailed))),
            "{reply:?}"
        );
    }
}
