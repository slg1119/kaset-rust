//! Moves WebView cookie reads and session store I/O off the async workers.
//!
//! `Webview::cookies()` blocks until the main thread answers (and deadlocks on
//! Windows when called from the main thread), and keyring calls block on the
//! OS store, so both run on Tauri's blocking pool.

use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::session::cookies::youtube_cookie_jar;
use crate::state::AppState;

/// Runs `work` on the blocking pool.
pub async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(AppError::from)
}

/// Reads the shared cookie store through the `label` window and passes the
/// YouTube cookies to `Session::refresh`. Runs at launch and after every
/// player page load. Returns true when the in-memory cookies changed.
pub async fn refresh_session_from_webview(
    app: &AppHandle,
    label: &str,
    reason: &'static str,
) -> bool {
    let Some(window) = app.get_webview_window(label) else {
        return false;
    };
    let task_app = app.clone();
    let result = blocking(move || {
        let cookies = window.cookies()?;
        let state = task_app.state::<AppState>();
        Ok::<_, tauri::Error>(state.session.refresh(youtube_cookie_jar(&cookies)))
    })
    .await;
    match result {
        Ok(Ok(changed)) => {
            if changed {
                tracing::info!(reason, "refreshed session cookies from the WebView");
            }
            changed
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, reason, "could not read WebView cookies");
            false
        }
        Err(error) => {
            tracing::warn!(%error, reason, "cookie refresh task failed");
            false
        }
    }
}
