//! The Google sign-in window.
//!
//! The window loads accounts.google.com and is done when a page on
//! music.youtube.com finishes loading. It has no capability, so pages in it
//! cannot call any command, and its navigation is limited to Google hosts.

use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_specta::Event;

use crate::error::AppError;
use crate::events::AuthChanged;
use crate::session::AuthStatus;
use crate::session::cookies::youtube_cookie_jar;
use crate::state::AppState;

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

/// Country Google domains such as `accounts.google.co.kr` or `google.com.au`,
/// which the sign-in flow can visit to set cookies.
fn is_country_google(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let Some(google) = labels.iter().rposition(|label| *label == "google") else {
        return false;
    };
    let is_country_code = |tld: &str| tld.len() == 2 && tld.chars().all(|c| c.is_ascii_lowercase());
    match &labels[google + 1..] {
        [tld] => is_country_code(tld),
        [second, tld] => matches!(*second, "co" | "com") && is_country_code(tld),
        _ => false,
    }
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
    let builder = WebviewWindowBuilder::new(app, LOGIN_LABEL, WebviewUrl::External(url))
        .title("Sign in to YouTube Music")
        .inner_size(500.0, 700.0)
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
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished && is_sign_in_landing(payload.url()) {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move { complete_sign_in(app).await });
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_USER_AGENT);
    builder.build()?;
    Ok(())
}

/// Reads the login window's cookies and signs in when SAPISID is present.
/// Runs on the async runtime: `cookies()` deadlocks on Windows when called
/// from the main thread or a synchronous handler.
async fn complete_sign_in(app: AppHandle) {
    let Some(window) = app.get_webview_window(LOGIN_LABEL) else {
        return;
    };
    let cookies = match window.cookies() {
        Ok(cookies) => cookies,
        Err(error) => {
            tracing::warn!(%error, "could not read login cookies");
            return;
        }
    };
    let jar = youtube_cookie_jar(&cookies);
    if jar.sapisid().is_none() {
        tracing::debug!("music.youtube.com loaded without SAPISID; waiting for sign-in");
        return;
    }
    let state = app.state::<AppState>();
    match state.session.sign_in(jar) {
        Ok(changed) => {
            if let Some(status) = changed {
                emit_auth_changed(&app, status);
            }
            close_login_window(&window);
        }
        Err(error) => tracing::warn!(%error, "sign-in failed"),
    }
}

fn close_login_window(window: &WebviewWindow) {
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
