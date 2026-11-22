use serde::{Deserialize, Serialize};

/// Sign-in state shown to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    /// No session. The UI shows the sign-in screen.
    SignedOut,
    /// Cookies with SAPISID are loaded and sent with API requests.
    SignedIn,
    /// The API answered 401/403. The UI asks the user to sign in again.
    Expired,
}
