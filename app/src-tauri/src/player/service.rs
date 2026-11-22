//! Applies engine outcomes: drives the player window, emits `player://state`
//! and `player://queue` to the main window.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::Mutex;

use super::engine::{Outcome, Player, PlayerState, QueueState, WebCommand};
use super::window;
use crate::error::AppError;
use crate::events::{PlayerStateChanged, QueueChanged};

/// Player events go to the main window. The player window's capability has
/// no event permissions, so its pages cannot listen to them either.
const UI_WINDOW: &str = "main";

pub struct PlayerService {
    // A tokio mutex, held while an outcome is applied, so window commands run
    // in the order the engine produced them. Never locked on the main thread.
    player: Mutex<Player>,
}

impl PlayerService {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1);
        Self {
            player: Mutex::new(Player::new(seed)),
        }
    }

    /// Runs `change` on the engine and applies its outcome.
    pub async fn update<F>(&self, app: &AppHandle, change: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>,
    {
        let mut player = self.player.lock().await;
        let outcome = change(&mut player, Instant::now())?;
        apply(app, outcome)
    }

    pub async fn snapshot(&self) -> (PlayerState, QueueState) {
        let player = self.player.lock().await;
        (player.state(), player.queue_state())
    }

    /// Stops playback and keeps player commands serialized until sign-out
    /// finishes, so they cannot reopen the window during cookie cleanup.
    pub(crate) async fn stop_for_sign_out<T>(
        &self,
        app: &AppHandle,
        sign_out: impl Future<Output = T>,
    ) -> T {
        let mut player = self.player.lock().await;
        if let Err(error) = apply(app, player.stop(Instant::now())) {
            tracing::warn!(%error, "could not stop the player during sign-out");
        }
        sign_out.await
    }
}

impl Default for PlayerService {
    fn default() -> Self {
        Self::new()
    }
}

fn apply(app: &AppHandle, outcome: Outcome) -> Result<(), AppError> {
    let result = match outcome.command {
        Some(WebCommand::Load(id)) => window::load(app, &id),
        Some(WebCommand::Play) => window::run_script(app, window::play_script()),
        Some(WebCommand::Pause) => window::run_script(app, window::pause_script()),
        Some(WebCommand::Seek(position)) => window::run_script(app, &window::seek_script(position)),
        Some(WebCommand::Stop) => {
            window::close(app);
            Ok(())
        }
        None => Ok(()),
    };
    if let Some(queue) = outcome.queue
        && let Err(error) = (QueueChanged { queue }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://queue");
    }
    if let Some(state) = outcome.state
        && let Err(error) = (PlayerStateChanged { state }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://state");
    }
    result
}
