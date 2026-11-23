//! The hidden player window: one WebView on music.youtube.com that plays the
//! current song. It shares the WebView cookie store with the login and main
//! windows, so it plays as the signed-in user.

use std::sync::{Arc, Mutex};

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::video_id::VideoId;
use crate::error::AppError;
use crate::session::cookies::{is_country_google, is_under};
use crate::state::AppState;
use crate::webview_session::{
    RestoredPlayerAuth, refresh_session_from_webview, restore_player_cookies,
};

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

#[derive(Default)]
struct Navigation {
    requested: Option<(Url, u64)>,
    starting: Option<u64>,
    committed: Option<(Url, u64)>,
}

impl Navigation {
    fn navigating(&mut self, url: &Url) {
        if let Some((_, generation)) = self.requested.take_if(|(watch, _)| watch == url) {
            self.starting = Some(generation);
        }
    }

    fn started(&mut self, url: &Url) {
        if url.as_str() == "about:blank" {
            return;
        }
        if let Some(generation) = self.starting.take() {
            self.committed = Some((url.clone(), generation));
        } else if let Some((committed_url, _)) = self.committed.as_mut() {
            // The watch page can redirect again after it committed.
            committed_url.clone_from(url);
        }
    }

    fn finished_outside_music(&mut self, url: &Url) -> Option<u64> {
        if is_music_page(url) {
            return None;
        }
        self.committed
            .take_if(|(committed_url, _)| committed_url == url)
            .map(|(_, generation)| generation)
    }
}

pub(super) struct PlayerWindow {
    window: WebviewWindow,
    navigation: Arc<Mutex<Navigation>>,
    restored_auth: Option<RestoredPlayerAuth>,
}

impl PlayerWindow {
    /// Create the player on a blank page so its own store can be seeded
    /// before the first request to YouTube.
    pub(super) fn new(app: &AppHandle) -> Result<Self, AppError> {
        let navigation = Arc::new(Mutex::new(Navigation::default()));
        let request_navigation = Arc::clone(&navigation);
        let page_navigation = Arc::clone(&navigation);
        let blank = Url::parse("about:blank").expect("constant URL is valid");
        let builder = WebviewWindowBuilder::new(app, PLAYER_LABEL, WebviewUrl::External(blank))
            .title("Kaset player")
            .inner_size(480.0, 360.0)
            .visible(false)
            .focused(false)
            .skip_taskbar(true)
            // macOS 14+: keep timers and media running while the window is hidden.
            .background_throttling(BackgroundThrottlingPolicy::Disabled)
            .initialization_script(PLAYER_SCRIPT)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_navigation(move |url| {
                let allowed = is_allowed_player_navigation(url);
                if allowed {
                    request_navigation
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .navigating(url);
                } else {
                    // Host only: query strings can carry tokens.
                    tracing::warn!(
                        scheme = url.scheme(),
                        host = url.host_str().unwrap_or(""),
                        "blocked player navigation"
                    );
                }
                allowed
            })
            .on_page_load(move |window, payload| {
                let url = payload.url();
                let mut navigation = page_navigation
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if payload.event() == PageLoadEvent::Started {
                    navigation.started(url);
                    return;
                }
                let generation = navigation.finished_outside_music(url);
                drop(navigation);
                let app = window.app_handle().clone();
                if is_music_page(url) {
                    tauri::async_runtime::spawn(async move {
                        refresh_session_from_webview(&app, PLAYER_LABEL, "player").await;
                    });
                } else if let Some(generation) = generation {
                    tauri::async_runtime::spawn(async move {
                        app.state::<AppState>()
                            .player
                            .redirected(&app, generation)
                            .await;
                    });
                }
            });
        // Same user agent as the login window, which shares this cookie store.
        #[cfg(target_os = "macos")]
        let builder = builder.user_agent(crate::login::SAFARI_USER_AGENT);
        let window = builder.build()?;
        Ok(Self {
            window,
            navigation,
            restored_auth: None,
        })
    }

    pub(super) async fn load(&mut self, id: &VideoId, generation: u64) -> Result<(), AppError> {
        self.restored_auth =
            restore_player_cookies(&self.window, self.restored_auth.take()).await?;
        let url = id.watch_url();
        self.navigation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requested = Some((url.clone(), generation));
        self.window.navigate(url)?;
        Ok(())
    }
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
    fn offsite_finish_is_attributed_to_the_committed_load_once() {
        let mut navigation = Navigation::default();
        let blank = Url::parse("about:blank").unwrap();
        let watch_a = VideoId::parse("aaaaaaaaaaa").unwrap().watch_url();
        let watch_b = VideoId::parse("bbbbbbbbbbb").unwrap().watch_url();
        let premium = Url::parse("https://www.youtube.com/musicpremium").unwrap();
        let consent = Url::parse("https://consent.youtube.com/").unwrap();
        navigation.requested = Some((watch_a.clone(), 1));
        navigation.started(&blank);
        assert_eq!(navigation.finished_outside_music(&blank), None);
        navigation.navigating(&watch_a);
        navigation.started(&watch_a);
        navigation.requested = Some((watch_b.clone(), 2));
        // A redirect from the first page can commit before the second request starts.
        navigation.navigating(&premium);
        navigation.started(&premium);
        assert_eq!(navigation.finished_outside_music(&premium), Some(1));
        assert_eq!(navigation.finished_outside_music(&premium), None);
        navigation.navigating(&watch_b);
        navigation.navigating(&consent);
        navigation.started(&consent);
        assert_eq!(navigation.finished_outside_music(&consent), Some(2));
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
