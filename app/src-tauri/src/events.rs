use serde::{Deserialize, Serialize};

use crate::session::AuthStatus;

/// Emitted on `auth://changed` whenever the sign-in state changes.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event,
)]
#[tauri_specta(event_name = "auth://changed")]
pub struct AuthChanged {
    pub status: AuthStatus,
}
