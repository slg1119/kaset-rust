use serde::{Deserialize, Serialize};

use crate::player::engine::{PlayerState, QueueState};
use crate::session::AuthStatus;

/// Emitted on `auth://changed` when the sign-in state or account session changes.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event,
)]
#[tauri_specta(event_name = "auth://changed")]
pub struct AuthChanged {
    pub status: AuthStatus,
}

/// Emitted to the main window on `player://state`: on every status, track, or
/// ad change, and at most four times a second for progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "player://state")]
pub struct PlayerStateChanged {
    pub state: PlayerState,
}

/// Emitted to the main window on `player://queue` when the queue or the
/// current position in it changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "player://queue")]
pub struct QueueChanged {
    pub queue: QueueState,
}
