#[derive(Debug, thiserror::Error)]
pub enum YtmError {
    /// HTTP 401/403, or cookies present without SAPISID. The user must sign in again.
    #[error("authentication expired")]
    AuthExpired,
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("unexpected HTTP status {status}")]
    Http { status: u16 },
    /// `path` is the dotted JSON path (or marker) that could not be read.
    #[error("could not parse {endpoint} response at {path}")]
    Parse { endpoint: String, path: String },
    #[error("rate limited")]
    RateLimited,
}
