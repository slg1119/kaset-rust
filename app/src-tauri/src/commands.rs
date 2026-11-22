pub mod feed;

use std::future::Future;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use ytm_core::client::{InnerTubeClient, playlist_browse_id};
use ytm_core::models::{Account, BrowsePage, SearchFilter, SearchResults};
use ytm_core::parsers::account::parse_account;
use ytm_core::parsers::browse_page::parse_browse_page;
use ytm_core::parsers::search::{parse_search, parse_search_continuation};
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, close_login_window, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::{blocking, clear_webview_cookies};
use crate::window_effects::WindowEffects;

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

/// Stops playback, forgets the stored session, and deletes the WebView's
/// Google cookies.
#[tauri::command]
#[specta::specta]
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        close_login_window(&window);
    }
    let result = state
        .player
        .stop_for_sign_out(&app, async {
            let task_app = app.clone();
            let result = blocking(move || {
                if let Some(window) = task_app.get_webview_window("main") {
                    clear_webview_cookies(&window, "sign-out");
                }
                task_app.state::<AppState>().session.sign_out()
            })
            .await;
            emit_auth_changed(&app, state.session.status());
            result
        })
        .await;
    result?
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
}

/// Fetches the first page of an album or playlist for the signed-in session.
#[tauri::command]
#[specta::specta]
pub async fn get_browse_page(
    app: AppHandle,
    state: State<'_, AppState>,
    browse_id: String,
) -> Result<BrowsePage, AppError> {
    checked_browse_id(&browse_id)?;
    let browse_id = playlist_browse_id(&browse_id);
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_browse_page(&client.browse(&browse_id).await?)?)
    })
    .await
}

fn checked_browse_id(id: &str) -> Result<(), AppError> {
    checked_browse_value(id, 2..=128, b"", "Invalid browse ID.")
}

fn checked_browse_value(
    value: &str,
    length: std::ops::RangeInclusive<usize>,
    extra: &[u8],
    message: &'static str,
) -> Result<(), AppError> {
    if length.contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') || extra.contains(&byte)
        })
    {
        Ok(())
    } else {
        Err(AppError::new(ErrorKind::InvalidArgument, message))
    }
}

/// The signed-in account's name, handle, and photo for the sidebar; `None`
/// when the response lists no named account.
#[tauri::command]
#[specta::specta]
pub async fn get_account(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<Account>, AppError> {
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_account(&client.accounts_list().await?))
    })
    .await
}

/// Longest accepted search query or suggestion input, in characters.
pub const MAX_QUERY_CHARS: usize = 200;

/// Searches YouTube Music with an optional filter. An empty query returns
/// no results without a request. Overlong queries return `invalid_argument`.
#[tauri::command]
#[specta::specta]
pub async fn search(
    app: AppHandle,
    state: State<'_, AppState>,
    query: String,
    filter: Option<SearchFilter>,
) -> Result<SearchResults, AppError> {
    let Some(query) = checked_query(&query)?.map(str::to_owned) else {
        return Ok(SearchResults {
            items: Vec::new(),
            continuation: None,
        });
    };
    signed_in_request(&app, &state, |client| async move {
        let response = match filter {
            Some(filter) => client.search_filtered(&query, filter).await?,
            None => client.search(&query).await?,
        };
        Ok(parse_search(&response))
    })
    .await
}

/// Fetches another search page. The UI tracks the filter for its cache, and
/// YouTube Music uses the filter already encoded in the continuation token.
#[tauri::command]
#[specta::specta]
pub async fn search_continuation(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
    filter: Option<SearchFilter>,
) -> Result<SearchResults, AppError> {
    let _ = filter;
    checked_search_token(&token)?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_search_continuation(
            &client.search_continuation(&token).await?,
        ))
    })
    .await
}

fn checked_search_token(token: &str) -> Result<(), AppError> {
    checked_browse_value(token, 1..=4096, b"%=+/.", "Invalid search continuation.")
}

/// Autocomplete suggestions for `input`. An empty input returns none
/// without a request.
#[tauri::command]
#[specta::specta]
pub async fn search_suggestions(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
) -> Result<Vec<String>, AppError> {
    let Some(input) = checked_query(&input)?.map(str::to_owned) else {
        return Ok(Vec::new());
    };
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_search_suggestions(
            &client.search_suggestions(&input).await?,
        ))
    })
    .await
}

/// The trimmed query; `None` when empty, `invalid_argument` when longer than
/// `MAX_QUERY_CHARS`.
fn checked_query(query: &str) -> Result<Option<&str>, AppError> {
    let query = query.trim();
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("Search text is limited to {MAX_QUERY_CHARS} characters."),
        ));
    }
    Ok((!query.is_empty()).then_some(query))
}

/// Runs an API request for the signed-in session. Waits for the launch cookie
/// refresh first, and expires the session on 401/403 only if it is still the
/// session the request started with.
async fn signed_in_request<T, F, Fut>(
    app: &AppHandle,
    state: &AppState,
    request: F,
) -> Result<T, AppError>
where
    F: FnOnce(Arc<InnerTubeClient>) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    if !state.launch.wait(LAUNCH_REFRESH_TIMEOUT).await {
        tracing::warn!("launch cookie refresh did not finish in time");
    }
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    let epoch = state.session.epoch();
    let result = async { request(state.client().await?).await }.await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(on_error(app, epoch, error).await),
    }
}

/// Whether native translucency is behind the sidebar (macOS vibrancy,
/// Windows 11 Mica). The UI paints a solid sidebar when it is not.
#[tauri::command]
#[specta::specta]
pub async fn window_vibrancy(effects: State<'_, WindowEffects>) -> Result<bool, AppError> {
    Ok(effects.vibrancy)
}

/// Moves the session to `Expired` on an auth error for a request that started
/// at `epoch`, and passes the error on.
pub(crate) async fn on_error(app: &AppHandle, epoch: u64, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired {
        let state = app.state::<AppState>();
        let mut player = state.player.lock_for_session_change().await;
        let task_app = app.clone();
        match blocking(move || task_app.state::<AppState>().session.mark_expired_if(epoch)).await {
            Ok(Some(status)) => {
                player.stop(app);
                emit_auth_changed(app, status);
            }
            Ok(None) => {}
            Err(task_error) => tracing::warn!(%task_error, "session expiry task failed"),
        }
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_browse_id_enforces_ascii_and_length() {
        let max = "a".repeat(128);
        for id in ["LL", "MPREb_test-id", &max] {
            assert!(checked_browse_id(id).is_ok());
        }

        let over = "a".repeat(129);
        for id in [
            "", "A", " VLtest", "VLtest\n", "a.b", "a/b", "a?b", "가a", &over,
        ] {
            assert_eq!(
                checked_browse_id(id).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
        }
    }

    #[test]
    fn checked_query_trims_and_skips_empty_text() {
        assert_eq!(checked_query("  lofi  ").unwrap(), Some("lofi"));
        assert_eq!(checked_query(" \t ").unwrap(), None);
    }

    #[test]
    fn checked_query_rejects_overlong_text() {
        let limit = "가".repeat(MAX_QUERY_CHARS);
        assert_eq!(checked_query(&limit).unwrap(), Some(limit.as_str()));
        let error = checked_query(&format!("{limit}a")).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidArgument);
    }

    #[test]
    fn search_continuation_enforces_the_opaque_token_contract() {
        for token in ["test_-%3D=+/.9", &"a".repeat(4096)] {
            assert!(checked_search_token(token).is_ok());
        }
        for token in [
            "",
            "test?token",
            "test token",
            "token\n",
            "가",
            &"a".repeat(4097),
        ] {
            assert_eq!(
                checked_search_token(token).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
        }
    }
}
