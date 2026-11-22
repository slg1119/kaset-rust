//! The hidden player window: one WebView on music.youtube.com that plays the
//! current song. It shares the WebView cookie store with the login and main
//! windows, so it plays as the signed-in user.

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use super::video_id::VideoId;
use crate::error::AppError;
use crate::session::cookies::{is_country_google, is_under};
use crate::webview_session::refresh_session_from_webview;

pub const PLAYER_LABEL: &str = "player";

/// The init script. It returns early outside https://music.youtube.com.
pub const PLAYER_SCRIPT: &str = include_str!("player.js");

const PLAY_SCRIPT: &str = "window.__kasetPlayer && window.__kasetPlayer.play()";
const PAUSE_SCRIPT: &str = "window.__kasetPlayer && window.__kasetPlayer.pause()";

/// Hosts the player window may load, as the top-level page or as frames
/// (wry on macOS asks for every frame). Only https://music.youtube.com pages
/// can call `playback_event`; the capability grants nothing to the others.
const ALLOWED_ROOTS: [&str; 10] = [
    "youtube.com",
    "google.com",
    "gstatic.com",
    "googlevideo.com",
    "ytimg.com",
    "ggpht.com",
    "googleusercontent.com",
    "youtube-nocookie.com",
    "doubleclick.net",
    "googlesyndication.com",
];

/// Navigation policy for the player window.
pub fn is_allowed_player_navigation(url: &Url) -> bool {
    match url.scheme() {
        "about" | "blob" | "data" => true,
        "https" => url.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            ALLOWED_ROOTS.iter().any(|root| is_under(&host, root)) || is_country_google(&host)
        }),
        _ => false,
    }
}

/// True for a page on https://music.youtube.com.
pub fn is_music_page(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some("music.youtube.com")
}

/// `eval` source that seeks to `position` seconds. `position` is finite and
/// non-negative (the engine sanitizes it), so it formats as a JS number.
pub fn seek_script(position: f64) -> String {
    let position = if position.is_finite() {
        position.max(0.0)
    } else {
        0.0
    };
    format!("window.__kasetPlayer && window.__kasetPlayer.seek({position})")
}

pub fn play_script() -> &'static str {
    PLAY_SCRIPT
}

pub fn pause_script() -> &'static str {
    PAUSE_SCRIPT
}

/// Loads `id`'s watch page in the player window, creating the hidden window
/// on first use. The init script starts playback once the page loads.
pub fn load(app: &AppHandle, id: &VideoId) -> Result<(), AppError> {
    let url = id.watch_url();
    if let Some(window) = app.get_webview_window(PLAYER_LABEL) {
        window.navigate(url)?;
        return Ok(());
    }
    let builder = WebviewWindowBuilder::new(app, PLAYER_LABEL, WebviewUrl::External(url))
        .title("Kaset player")
        .inner_size(480.0, 360.0)
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        // macOS 14+: keep timers and media running while the window is hidden.
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .initialization_script(PLAYER_SCRIPT)
        .on_navigation(|url| {
            let allowed = is_allowed_player_navigation(url);
            if !allowed {
                // Host only: query strings can carry tokens.
                tracing::warn!(
                    scheme = url.scheme(),
                    host = url.host_str().unwrap_or(""),
                    "blocked player navigation"
                );
            }
            allowed
        })
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished && is_music_page(payload.url()) {
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    refresh_session_from_webview(&app, PLAYER_LABEL, "player").await;
                });
            }
        });
    // Same user agent as the login window, which shares this cookie store.
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(crate::login::SAFARI_USER_AGENT);
    builder.build()?;
    Ok(())
}

/// Runs one of the scripts above in the player window, if it exists.
pub fn run_script(app: &AppHandle, script: &str) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(PLAYER_LABEL) {
        window.eval(script)?;
    }
    Ok(())
}

/// Closes the player window, which stops audio.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PLAYER_LABEL)
        && let Err(error) = window.close()
    {
        tracing::warn!(%error, "could not close the player window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_allowed_player_navigation(&url.parse().unwrap())
    }

    #[test]
    fn allows_youtube_music_and_its_media_hosts() {
        assert!(allowed("https://music.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(allowed("https://accounts.youtube.com/RotateCookiesPage"));
        assert!(allowed("https://consent.youtube.com/m"));
        assert!(allowed("https://accounts.google.com/ServiceLogin"));
        assert!(allowed("https://accounts.google.co.kr/"));
        assert!(allowed(
            "https://rr1---sn-test.googlevideo.com/videoplayback"
        ));
        assert!(allowed("https://i.ytimg.com/vi/x/hqdefault.jpg"));
        assert!(allowed("https://googleads.g.doubleclick.net/pagead/id"));
        assert!(allowed("about:blank"));
        assert!(allowed("blob:https://music.youtube.com/0f3c"));
    }

    #[test]
    fn blocks_everything_else() {
        assert!(!allowed("http://music.youtube.com/"));
        assert!(!allowed("https://example.com/"));
        assert!(!allowed("https://music.youtube.com.evil.example/"));
        assert!(!allowed("https://notyoutube.com/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed("tauri://localhost/"));
    }

    #[test]
    fn music_pages_are_https_music_youtube_com() {
        assert!(is_music_page(
            &"https://music.youtube.com/watch?v=x".parse().unwrap()
        ));
        assert!(!is_music_page(&"https://www.youtube.com/".parse().unwrap()));
        assert!(!is_music_page(
            &"http://music.youtube.com/".parse().unwrap()
        ));
    }

    #[test]
    fn seek_script_formats_a_plain_number() {
        assert_eq!(
            seek_script(12.5),
            "window.__kasetPlayer && window.__kasetPlayer.seek(12.5)"
        );
        assert_eq!(
            seek_script(0.0),
            "window.__kasetPlayer && window.__kasetPlayer.seek(0)"
        );
        assert_eq!(
            seek_script(f64::NAN),
            "window.__kasetPlayer && window.__kasetPlayer.seek(0)"
        );
        assert_eq!(
            seek_script(-4.0),
            "window.__kasetPlayer && window.__kasetPlayer.seek(0)"
        );
    }

    #[test]
    fn init_script_is_guarded_to_music_youtube_com_and_uses_one_command() {
        assert!(PLAYER_SCRIPT.contains("location.origin !== 'https://music.youtube.com'"));
        assert!(PLAYER_SCRIPT.contains("window.top !== window"));
        assert_eq!(PLAYER_SCRIPT.matches("invoke(").count(), 1);
        assert!(PLAYER_SCRIPT.contains("invoke('playback_event'"));
    }
}
