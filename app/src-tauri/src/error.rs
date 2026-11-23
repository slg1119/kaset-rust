use serde::Serialize;
use ytm_core::YtmError;

/// What went wrong, for the UI to branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    AuthExpired,
    NotSignedIn,
    /// A command argument was rejected, such as an unplayable queue position.
    InvalidArgument,
    Network,
    Http,
    PodcastsUnavailable,
    Parse,
    RateLimited,
    Internal,
}

/// Error returned by every command. Serializes as `{ kind, message }`.
/// `message` is for display and never contains cookies or headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    pub fn not_signed_in() -> Self {
        Self::new(ErrorKind::NotSignedIn, "Sign in to continue.")
    }
}

impl From<YtmError> for AppError {
    fn from(error: YtmError) -> Self {
        let kind = match &error {
            YtmError::AuthExpired => ErrorKind::AuthExpired,
            YtmError::Network(_) => ErrorKind::Network,
            YtmError::Http { .. } => ErrorKind::Http,
            YtmError::Parse { .. } => ErrorKind::Parse,
            YtmError::RateLimited => ErrorKind::RateLimited,
        };
        Self::new(kind, error.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(error: tauri::Error) -> Self {
        Self::internal(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ytm_errors_to_kinds() {
        assert_eq!(
            AppError::from(YtmError::AuthExpired).kind,
            ErrorKind::AuthExpired
        );
        assert_eq!(
            AppError::from(YtmError::Http { status: 500 }).kind,
            ErrorKind::Http
        );
        assert_eq!(
            AppError::from(YtmError::RateLimited).kind,
            ErrorKind::RateLimited
        );
        let parse = AppError::from(YtmError::Parse {
            endpoint: "browse".into(),
            path: "contents".into(),
        });
        assert_eq!(parse.kind, ErrorKind::Parse);
        assert_eq!(parse.message, "could not parse browse response at contents");
    }

    #[test]
    fn serializes_as_kind_and_message() {
        let json = serde_json::to_value(AppError::not_signed_in()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "not_signed_in", "message": "Sign in to continue." })
        );
    }
}
