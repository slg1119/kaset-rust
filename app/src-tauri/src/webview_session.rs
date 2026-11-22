//! Moves WebView cookie reads and session store I/O off the async workers.
//!
//! `Webview::cookies()` blocks until the main thread answers (and deadlocks on
//! Windows when called from the main thread), and keyring calls block on the
//! OS store, so both run on Tauri's blocking pool.

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::error::AppError;
use crate::session::cookies::{session_cookies, youtube_cookie_jar};
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

/// Deletes YouTube and Google session cookies from the shared WebView store,
/// re-reading up to three times like the Swift app's cookie clearer. Call on
/// the blocking pool.
pub(crate) fn clear_webview_cookies(window: &WebviewWindow, reason: &'static str) {
    for _ in 0..3 {
        let remaining = match window.cookies() {
            Ok(cookies) => session_cookies(cookies),
            Err(error) => {
                tracing::warn!(%error, reason, "could not read cookies for WebView cleanup");
                return;
            }
        };
        if remaining.is_empty() {
            return;
        }
        for cookie in remaining {
            if let Err(error) = window.delete_cookie(cookie) {
                tracing::warn!(%error, reason, "could not delete a WebView cookie");
            }
        }
    }
    tracing::warn!(reason, "Google cookies remained after WebView cleanup");
}

/// Reads the shared cookie store through the `label` window and passes the
/// YouTube cookies to `Session::refresh_if`. Runs at launch and after every
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
        let state = task_app.state::<AppState>();
        let epoch = state.session.epoch();
        let cookies = window.cookies()?;
        Ok::<_, tauri::Error>(
            state
                .session
                .refresh_if(epoch, youtube_cookie_jar(&cookies)),
        )
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
