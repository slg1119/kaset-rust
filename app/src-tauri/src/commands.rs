use tauri::{AppHandle, Manager, State};
use ytm_core::models::HomePage;
use ytm_core::parsers::home::parse_home;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, close_login_window, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::{blocking, clear_webview_cookies};

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
        if let Some(window) = task_app.get_webview_window("main") {
            clear_webview_cookies(&window, "sign-out");
        }
        task_app.state::<AppState>().session.sign_out()
    })
    .await;
    emit_auth_changed(&app, app.state::<AppState>().session.status());
    result?
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
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
    match result {
        Ok(page) => Ok(page),
        Err(error) => Err(on_error(&app, epoch, error).await),
    }
}

/// Moves the session to `Expired` on an auth error for a request that started
/// at `epoch`, and passes the error on.
async fn on_error(app: &AppHandle, epoch: u64, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired {
        let task_app = app.clone();
        match blocking(move || task_app.state::<AppState>().session.mark_expired_if(epoch)).await {
            Ok(Some(status)) => emit_auth_changed(app, status),
            Ok(None) => {}
            Err(task_error) => tracing::warn!(%task_error, "session expiry task failed"),
        }
    }
    error
}
