use tauri::{AppHandle, Manager, State};
use ytm_core::models::HomePage;
use ytm_core::parsers::home::parse_home;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, close_login_window, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::session::cookies::session_cookies;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::blocking;

/// The current sign-in state. The UI calls this once at start and then
/// follows `auth://changed`.
#[tauri::command]
#[specta::specta]
pub async fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, AppError> {
    Ok(state.session.status())
}

/// Opens the Google sign-in window. Completion arrives as `auth://changed`.
#[tauri::command]
#[specta::specta]
pub async fn sign_in(app: AppHandle) -> Result<(), AppError> {
    open_login_window(&app)
}

/// Forgets the stored session and deletes the WebView's Google cookies.
#[tauri::command]
#[specta::specta]
pub async fn sign_out(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        close_login_window(&window);
    }
    let task_app = app.clone();
    let result = blocking(move || {
        let result = task_app.state::<AppState>().session.sign_out();
        clear_webview_cookies(&task_app);
        result
    })
    .await?;
    emit_auth_changed(&app, AuthStatus::SignedOut);
    result
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
}

/// Deletes YouTube and Google session cookies from the shared WebView store,
/// re-reading up to three times like the Swift app's cookie clearer. Blocking:
/// call it on the blocking pool.
fn clear_webview_cookies(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    for _ in 0..3 {
        let remaining = match window.cookies() {
            Ok(cookies) => session_cookies(cookies),
            Err(error) => {
                tracing::warn!(%error, "could not read cookies during sign-out");
                return;
            }
        };
        if remaining.is_empty() {
            return;
        }
        for cookie in remaining {
            if let Err(error) = window.delete_cookie(cookie) {
                tracing::warn!(%error, "could not delete a cookie during sign-out");
            }
        }
    }
    tracing::warn!("Google cookies remained in the WebView after sign-out");
}

/// Fetches and parses the signed-in home feed. Waits for the launch cookie
/// refresh first, and expires the session on 401/403 only if it is still the
/// session the request started with.
#[tauri::command]
#[specta::specta]
pub async fn get_home(app: AppHandle, state: State<'_, AppState>) -> Result<HomePage, AppError> {
    if !state.launch.wait(LAUNCH_REFRESH_TIMEOUT).await {
        tracing::warn!("launch cookie refresh did not finish in time");
    }
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    let epoch = state.session.epoch();
    let result = async {
        let client = state.client().await?;
        let response = client.browse("FEmusic_home").await?;
        Ok::<_, AppError>(parse_home(&response)?)
    }
    .await;
    result.map_err(|error| on_error(&app, &state, epoch, error))
}

/// Moves the session to `Expired` on an auth error for a request that started
/// at `epoch`, and passes the error on.
fn on_error(app: &AppHandle, state: &AppState, epoch: u64, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired
        && let Some(status) = state.session.mark_expired_if(epoch)
    {
        emit_auth_changed(app, status);
    }
    error
}
