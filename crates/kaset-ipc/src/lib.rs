//! Fixed, versioned protocol for the desktop player and its local CLI.
use serde::{Deserialize, Serialize};

mod transport;
pub use transport::{ClientError, request, serve};

pub const VIDEO_ID_LEN: usize = 11;
pub const MAX_REQUEST_BYTES: usize = 4 * 1024;
const VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Status {},
    PlayPause {},
    Play {},
    Pause {},
    Next {},
    Previous {},
    Seek { seconds: f64 },
    Queue {},
    PlayVideo { video_id: String },
    PlayPage { browse_id: String },
    Shuffle { on: bool },
    Repeat { mode: RepeatMode },
}

#[derive(Serialize, Deserialize)]
struct Request<T> {
    v: u8,
    #[serde(flatten)]
    command: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    Idle,
    Loading,
    Playing,
    Paused,
    Ended,
}

impl PlaybackStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Ended => "ended",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub title: String,
    pub artists: Vec<String>,
    pub video_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    #[serde(flatten)]
    pub track: Option<Track>,
    pub position: f64,
    pub duration: f64,
    pub status: PlaybackStatus,
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    pub songs: Vec<Track>,
    pub current: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Status { status: Status },
    Queue { queue: Queue },
    Error { error: Error },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    #[error("Invalid playback request.")]
    InvalidRequest,
    #[error("Unsupported playback protocol version.")]
    UnsupportedVersion,
    #[error("Playback request exceeds 4 KiB.")]
    RequestTooLarge,
    #[error("Not signed in. Sign in with the Kaset app first.")]
    NotSignedIn,
    #[error("Session expired. Open the Kaset app to sign in again.")]
    AuthExpired,
    #[error("Kaset could not play that request.")]
    PlaybackFailed,
}

pub fn valid_video_id(value: &str) -> bool {
    value.len() == VIDEO_ID_LEN && id_chars(value)
}

pub fn valid_browse_id(value: &str) -> bool {
    (2..=128).contains(&value.len()) && id_chars(value)
}

pub fn valid_seek(seconds: f64) -> bool {
    seconds.is_finite() && seconds >= 0.0
}

fn id_chars(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate(command: &Command) -> Result<(), Error> {
    match command {
        Command::PlayVideo { video_id } if !valid_video_id(video_id) => Err(Error::InvalidRequest),
        Command::PlayPage { browse_id } if !valid_browse_id(browse_id) => {
            Err(Error::InvalidRequest)
        }
        Command::Seek { seconds } if !valid_seek(*seconds) => Err(Error::InvalidRequest),
        _ => Ok(()),
    }
}

fn encode_request(command: &Command) -> Result<Vec<u8>, Error> {
    validate(command)?;
    let mut bytes = serde_json::to_vec(&Request {
        v: VERSION,
        command,
    })
    .map_err(|_| Error::InvalidRequest)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(Error::RequestTooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

fn decode_request(bytes: &[u8]) -> Result<Command, Error> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(Error::RequestTooLarge);
    }
    let request: Request<Command> =
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
    if request.v != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    validate(&request.command)?;
    Ok(request.command)
}

#[cfg(test)]
mod tests;
