//! The Google sign-in window.
//!
//! The window loads accounts.google.com and is done when a page on
//! music.youtube.com finishes loading. It has no capability, so pages in it
//! cannot call any command, and its navigation is limited to Google hosts.

use std::sync::Arc;

use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_specta::Event;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::events::AuthChanged;
use crate::session::cookies::{is_country_google, youtube_cookie_jar};
use crate::session::{AuthStatus, SessionError};
use crate::state::AppState;
use crate::webview_session::{blocking, clear_webview_cookies};

pub const LOGIN_LABEL: &str = "login";

/// Same entry point as the Swift app's `LoginWebView`: sign in, then land on
/// music.youtube.com through www.youtube.com/signin.
pub const LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&uilel=3&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26app%3Ddesktop%26hl%3Den%26next%3Dhttps%253A%252F%252Fmusic.youtube.com%252F";

/// Google rejects sign-in from WKWebView's default user agent. The spike signed
/// in with this Safari user agent without a "browser may not be secure" block.
#[cfg(target_os = "macos")]
pub const SAFARI_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

const ALLOWED_ROOTS: [&str; 6] = [
    "google.com",
    "youtube.com",
    "gstatic.com",
    "googleusercontent.com",
    "googleapis.com",
    "youtube-nocookie.com",
];

fn is_under(host: &str, root: &str) -> bool {
    host == root || host.ends_with(&format!(".{root}"))
}

/// Navigation policy for the login window. On macOS wry asks for every frame,
/// not only the main frame, so `about:`, `blob:`, and `data:` iframes must pass.
pub fn is_allowed_login_navigation(url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" => true,
        "https" => url.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            ALLOWED_ROOTS.iter().any(|root| is_under(&host, root)) || is_country_google(&host)
        }),
        _ => false,
    }
}

/// True for a finished load of a music.youtube.com page: sign-in is complete.
pub fn is_sign_in_landing(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("music.youtube.com")
}

/// Opens the login window, or focuses it if it is already open.
pub fn open_login_window(app: &AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        window.set_focus()?;
        return Ok(());
    }
    let url: Url = LOGIN_URL
        .parse()
        .map_err(|_| AppError::internal("invalid login URL"))?;
    let completions = Arc::new(Mutex::new(LoginCompletion::default()));
    let builder = WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(url))
        .title("Sign in to YouTube Music")
        .inner_size(500.0, 700.0)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_navigation(|url| {
            let allowed = is_allowed_login_navigation(url);
            if !allowed {
                // Host only: query strings in sign-in URLs can carry tokens.
                tracing::warn!(
                    scheme = url.scheme(),
                    host = url.host_str().unwrap_or(""),
                    "blocked login navigation"
                );
            }
            allowed
        })
        .on_page_load(move |window, payload| {
            if payload.event() == PageLoadEvent::Finished && is_sign_in_landing(payload.url()) {
                let app = window.app_handle().clone();
                // Captured before the cookie read: a sign-out in between makes
                // this completion stale.
                let epoch = app.state::<AppState>().session.epoch();
                let completions = Arc::clone(&completions);
                tauri::async_runtime::spawn(async move {
                    // Duplicate landings must not race the expired-cookie cleanup.
                    let mut completion = completions.lock().await;
                    if !completion.closed {
                        complete_sign_in(app, window, epoch, &mut completion).await;
                    }
                });
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_USER_AGENT);
    builder.build()?;
    Ok(())
}

#[derive(Default)]
struct LoginCompletion {
    retried: bool,
    closed: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum SignInAction {
    Complete(Option<AuthStatus>),
    RetryInteractively,
    CloseAfterRetry,
}

fn sign_in_action(
    result: Result<Option<AuthStatus>, SessionError>,
    retried: &mut bool,
) -> Result<SignInAction, SessionError> {
    match result {
        Ok(changed) => Ok(SignInAction::Complete(changed)),
        Err(SessionError::SameAsExpired) => {
            if std::mem::replace(retried, true) {
                Ok(SignInAction::CloseAfterRetry)
            } else {
                Ok(SignInAction::RetryInteractively)
            }
        }
        Err(error) => Err(error),
    }
}

fn interactive_login_url() -> Result<Url, AppError> {
    LOGIN_URL
        .replacen("&passive=true", "", 1)
        .parse()
        .map_err(|_| AppError::internal("invalid interactive login URL"))
}

/// Reads the login window's cookies and signs in when SAPISID is present and
/// the session has not changed since `epoch`. The cookie read and the store
/// save run on the blocking pool.
async fn complete_sign_in(
    app: AppHandle,
    window: WebviewWindow,
    epoch: u64,
    completion: &mut LoginCompletion,
) {
    let state = app.state::<AppState>();
    let mut player = state.player.lock_for_session_change().await;
    let reader = window.clone();
    let task_app = app.clone();
    let result = blocking(move || {
        let state = task_app.state::<AppState>();
        if state.session.epoch() != epoch {
            return Ok(Err(SessionError::Stale));
        }
        let cookies = reader.cookies()?;
        Ok::<_, tauri::Error>(
            state
                .session
                .sign_in_if(epoch, youtube_cookie_jar(&cookies)),
        )
    })
    .await;
    let result = match result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            tracing::warn!(%error, "could not read login cookies");
            return;
        }
        Err(error) => {
            tracing::warn!(%error, "sign-in task failed");
            return;
        }
    };
    match sign_in_action(result, &mut completion.retried) {
        Ok(SignInAction::Complete(changed)) => {
            if let Some(status) = changed {
                emit_auth_changed(&app, status);
            }
            completion.closed = true;
            close_login_window(&window);
        }
        Err(SessionError::NoSapisid) => {
            tracing::debug!("music.youtube.com loaded without SAPISID; waiting for sign-in");
        }
        Ok(SignInAction::RetryInteractively) => {
            player.stop(&app);
            let reader = window.clone();
            if let Err(error) =
                blocking(move || clear_webview_cookies(&reader, "expired sign-in")).await
            {
                tracing::warn!(%error, "expired sign-in cookie cleanup task failed");
            }
            match interactive_login_url()
                .and_then(|url| window.navigate(url).map_err(AppError::from))
            {
                Ok(()) => {
                    tracing::info!("reopened Google sign-in after rejecting expired cookies")
                }
                Err(error) => tracing::warn!(%error, "could not reopen Google sign-in"),
            }
        }
        Ok(SignInAction::CloseAfterRetry) => {
            tracing::warn!("closing login window after expired cookies persisted through retry");
            completion.closed = true;
            close_login_window(&window);
        }
        Err(SessionError::Stale) => {
            tracing::debug!("discarded a sign-in that started before a session change");
        }
        Err(error) => tracing::warn!(%error, "sign-in failed"),
    }
}

pub(crate) fn close_login_window(window: &WebviewWindow) {
    if let Err(error) = window.close() {
        tracing::warn!(%error, "could not close the login window");
    }
}

pub fn emit_auth_changed(app: &AppHandle, status: AuthStatus) {
    if let Err(error) = (AuthChanged { status }).emit(app) {
        tracing::warn!(%error, "could not emit auth://changed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_allowed_login_navigation(&url.parse().unwrap())
    }

    #[test]
    fn allows_google_sign_in_hosts() {
        assert!(allowed(LOGIN_URL));
        assert!(allowed("https://accounts.youtube.com/accounts/SetSID"));
        assert!(allowed(
            "https://www.youtube.com/signin?action_handle_signin=true"
        ));
        assert!(allowed("https://music.youtube.com/"));
        assert!(allowed("https://consent.youtube.com/m"));
        assert!(allowed("https://www.google.com/recaptcha/api2/anchor"));
        assert!(allowed("https://ssl.gstatic.com/accounts/static.js"));
        assert!(allowed("https://accounts.google.co.kr/accounts/SetSID"));
        assert!(allowed("https://www.google.com.au/"));
        assert!(allowed("https://accounts.google.de/"));
    }

    #[test]
    fn expired_cookies_restart_sign_in_without_passive_mode() {
        let mut retried = false;
        assert_eq!(
            sign_in_action(Err(SessionError::SameAsExpired), &mut retried).unwrap(),
            SignInAction::RetryInteractively,
        );
        assert!(matches!(
            sign_in_action(Err(SessionError::Stale), &mut retried),
            Err(SessionError::Stale)
        ));

        let interactive = interactive_login_url().unwrap();
        assert!(is_allowed_login_navigation(&interactive));
        let passive: Url = LOGIN_URL.parse().unwrap();
        assert_eq!(interactive.host_str(), passive.host_str());
        assert_eq!(interactive.path(), passive.path());
        let expected: Vec<_> = passive
            .query_pairs()
            .filter(|(name, _)| name != "passive")
            .collect();
        assert_eq!(interactive.query_pairs().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn expired_cookie_retry_is_limited_to_once_per_window() {
        let mut retried = false;
        assert_eq!(
            sign_in_action(Err(SessionError::SameAsExpired), &mut retried).unwrap(),
            SignInAction::RetryInteractively,
        );
        assert_eq!(
            sign_in_action(Err(SessionError::SameAsExpired), &mut retried).unwrap(),
            SignInAction::CloseAfterRetry,
        );
        let mut other_window = false;
        assert_eq!(
            sign_in_action(Err(SessionError::SameAsExpired), &mut other_window).unwrap(),
            SignInAction::RetryInteractively,
        );
    }

    #[test]
    fn allows_frame_schemes_without_a_host() {
        assert!(allowed("about:blank"));
        assert!(allowed("about:srcdoc"));
        assert!(allowed("blob:https://accounts.google.com/0f3c"));
        assert!(allowed("data:text/html,hi"));
    }

    #[test]
    fn blocks_everything_else() {
        assert!(!allowed("http://accounts.google.com/"));
        assert!(!allowed("https://example.com/"));
        assert!(!allowed("https://google.com.evil.example/"));
        assert!(!allowed("https://youtube.com.evil.example/"));
        assert!(!allowed("https://notgoogle.com/"));
        assert!(!allowed("https://google.evil/"));
        assert!(!allowed("https://google.co.evil/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed("tauri://localhost/"));
    }

    #[test]
    fn sign_in_lands_on_music_youtube_com() {
        assert!(is_sign_in_landing(
            &"https://music.youtube.com/".parse().unwrap()
        ));
        assert!(is_sign_in_landing(
            &"https://music.youtube.com/library".parse().unwrap()
        ));
        assert!(!is_sign_in_landing(
            &"https://www.youtube.com/".parse().unwrap()
        ));
        assert!(!is_sign_in_landing(&LOGIN_URL.parse().unwrap()));
        assert!(!is_sign_in_landing(
            &"http://music.youtube.com/".parse().unwrap()
        ));
    }
}
