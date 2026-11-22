use std::sync::Arc;

use tokio::sync::OnceCell;
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};

use crate::error::AppError;
use crate::session::Session;

/// Managed Tauri state.
pub struct AppState {
    pub session: Session,
    client: OnceCell<Arc<InnerTubeClient>>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            client: OnceCell::new(),
        }
    }

    /// The API client, built on first use. It shares the session's cookie
    /// handle, so sign-in and sign-out take effect without rebuilding it.
    /// `KASET_YTMUSIC_API_KEY` overrides the key fetched from the web client.
    pub async fn client(&self) -> Result<Arc<InnerTubeClient>, AppError> {
        self.client
            .get_or_try_init(|| async {
                let api_key = match std::env::var(API_KEY_ENV) {
                    Ok(key) if !key.trim().is_empty() => key,
                    _ => fetch_api_key(MUSIC_ORIGIN).await?,
                };
                let client = InnerTubeClient::with_shared_cookies(
                    ClientConfig::music(api_key),
                    self.session.cookies().clone(),
                )?;
                Ok::<_, AppError>(Arc::new(client))
            })
            .await
            .cloned()
    }
}
