use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(target_os = "macos")]
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

const INIT_SCRIPT: &str = r#"
(function () {
  if (location.hostname !== 'music.youtube.com') return;
  const send = (payload) => {
    console.log('[spike]', JSON.stringify(payload));
    try { window.__TAURI_INTERNALS__.invoke('playback_event', { payload }); }
    catch (e) { console.log('[spike] invoke failed', String(e)); }
  };
  const probe = async (keySystem) => {
    try {
      await navigator.requestMediaKeySystemAccess(keySystem, [{
        initDataTypes: ['cenc', 'sinf', 'skd'],
        audioCapabilities: [{ contentType: 'audio/mp4; codecs="mp4a.40.2"' }],
      }]);
      return true;
    } catch (e) { return String((e && e.name) || e); }
  };
  window.addEventListener('load', async () => {
    send({
      event: 'eme',
      widevine: await probe('com.widevine.alpha'),
      fairplay: await probe('com.apple.fps'),
      playready: await probe('com.microsoft.playready'),
    });
  });
  let last = 0;
  setInterval(() => {
    const v = document.querySelector('video');
    if (!v || v.__spikeHooked) return;
    v.__spikeHooked = true;
    ['play', 'pause', 'ended', 'loadedmetadata', 'error'].forEach((name) =>
      v.addEventListener(name, () => send({ event: name, t: v.currentTime, d: v.duration })));
    v.addEventListener('timeupdate', () => {
      const now = Date.now();
      if (now - last > 1000) { last = now; send({ event: 'timeupdate', t: v.currentTime, d: v.duration }); }
    });
    send({ event: 'hooked', paused: v.paused });
  }, 500);
})();
"#;

#[derive(serde::Serialize)]
struct CookieInfo {
    window: String,
    name: String,
    domain: Option<String>,
    http_only: Option<bool>,
}

#[tauri::command]
async fn open_login(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("login") {
        return w.set_focus().map_err(|e| e.to_string());
    }
    let url = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F"
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    let builder = WebviewWindowBuilder::new(&app, "login", WebviewUrl::External(url))
        .title("Spike login")
        .inner_size(500.0, 700.0);
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_UA);
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn open_player(app: AppHandle, video_id: String, visible: bool) -> Result<(), String> {
    let url = format!("https://music.youtube.com/watch?v={video_id}")
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("player") {
        w.navigate(url).map_err(|e| e.to_string())?;
        let shown = if visible { w.show() } else { w.hide() };
        return shown.map_err(|e| e.to_string());
    }
    let builder = WebviewWindowBuilder::new(&app, "player", WebviewUrl::External(url))
        .title("Spike player")
        .inner_size(900.0, 700.0)
        .visible(visible)
        .initialization_script(INIT_SCRIPT);
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_UA);
    #[cfg(windows)]
    let builder = builder.additional_browser_args("--autoplay-policy=no-user-gesture-required");
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// Prints cookie names and flags only. Never returns or logs values.
#[tauri::command]
async fn dump_cookies(app: AppHandle) -> Result<Vec<CookieInfo>, String> {
    let url = "https://music.youtube.com/"
        .parse::<tauri::Url>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for label in ["login", "player"] {
        let Some(w) = app.get_webview_window(label) else {
            continue;
        };
        for c in w.cookies_for_url(url.clone()).map_err(|e| e.to_string())? {
            out.push(CookieInfo {
                window: label.into(),
                name: c.name().into(),
                domain: c.domain().map(str::to_owned),
                http_only: c.http_only(),
            });
        }
    }
    Ok(out)
}

#[tauri::command]
fn playback_event(payload: serde_json::Value) {
    println!("[playback_event] {payload}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open_login,
            open_player,
            dump_cookies,
            playback_event
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
