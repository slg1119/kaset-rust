use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, OnceCell, watch};
use ytm_core::api_key::music_api_key;
use ytm_core::client::{ClientConfig, InnerTubeClient};

use crate::commands::account_actions::FeedbackCache;
use crate::error::AppError;
use crate::player::service::PlayerService;
use crate::session::Session;

/// How long API commands wait for the launch cookie refresh.
pub const LAUNCH_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens once the launch cookie refresh has finished, so the first API request
/// uses rotated cookies instead of expiring the session with stale stored ones.
pub struct LaunchGate {
    open: watch::Sender<bool>,
}

impl LaunchGate {
    pub fn new() -> Self {
        Self {
            open: watch::Sender::new(false),
        }
    }

    pub fn open(&self) {
        self.open.send_replace(true);
    }

    /// True once the gate is open; false if `timeout` passes first.
    pub async fn wait(&self, timeout: Duration) -> bool {
        let mut open = self.open.subscribe();
        tokio::time::timeout(timeout, open.wait_for(|open| *open))
            .await
            .is_ok_and(|result| result.is_ok())
    }
}

impl Default for LaunchGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Managed Tauri state.
pub struct AppState {
    pub session: Session,
    pub launch: LaunchGate,
    pub player: PlayerService,
    pub(crate) feedback: Mutex<FeedbackCache>,
    client: OnceCell<Arc<InnerTubeClient>>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            launch: LaunchGate::new(),
            player: PlayerService::new(),
            feedback: Mutex::default(),
            client: OnceCell::new(),
        }
    }

    /// The API client, built on first use. It shares the session's cookie
    /// handle, so sign-in and sign-out take effect without rebuilding it.
    /// `KASET_YTMUSIC_API_KEY` overrides the key fetched from the web client.
    pub async fn client(&self) -> Result<Arc<InnerTubeClient>, AppError> {
        self.client
            .get_or_try_init(|| async {
                let client = InnerTubeClient::with_shared_cookies(
                    ClientConfig::music(music_api_key().await?),
                    self.session.cookies().clone(),
                )?;
                Ok::<_, AppError>(Arc::new(client))
            })
            .await
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn launch_gate_waits_until_opened() {
        let gate = Arc::new(LaunchGate::new());
        let waiter = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.wait(Duration::from_secs(5)).await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        gate.open();
        assert!(waiter.await.unwrap());
    }

    #[tokio::test]
    async fn launch_gate_returns_at_once_when_open() {
        let gate = LaunchGate::new();
        gate.open();
        assert!(gate.wait(Duration::from_millis(1)).await);
    }

    #[tokio::test]
    async fn launch_gate_times_out() {
        assert!(!LaunchGate::new().wait(Duration::from_millis(10)).await);
    }
}
