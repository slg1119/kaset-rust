# Spike: Tauri playback feasibility (2026-11-22)

Throwaway code: `spikes/tauri-playback/`. Tauri 2.12.7, wry 0.57.3, macOS 27.2.1.
Checks run by a human with a YouTube Music Premium account (region: Korea, where
YouTube Music requires Premium).

| Risk | macOS result | Windows result | Evidence |
|------|--------------|----------------|----------|
| 1. HttpOnly cookies readable from Rust | **Pass** | pending hardware | `webview.cookies()` returned `__Secure-1PSID`, `__Secure-3PSID`, `HSID`, `SSID`, `LOGIN_INFO`, … with `http_only: true`, plus `SAPISID` and `__Secure-3PAPISID`. Names only were printed. |
| 2. Hidden WebView autoplays without a gesture | **Pass** (with host-initiated `video.play()`) | pending hardware | `{"event":"autoplay","ok":true}` followed by `timeupdate` advancing every second; audible with the player window hidden, no click inside it. |
| 3. Premium playback in the WebView | **Pass** | pending hardware | Audio played, no ads (Premium recognized). EME probe: `navigator.requestMediaKeySystemAccess` is `undefined` in WKWebView; legacy `WebKitMediaKeys` is a `function`. Playback works anyway — same as the Swift app, which sets no DRM options on its WKWebView. |
| 4. Google sign-in in an embedded WebView | **Pass** | pending hardware | Signed in through `accounts.google.com` in a WKWebView with a Safari user agent; no "browser may not be secure" block. Player window showed the signed-in avatar and personalized shelves. |

Remote IPC (`player-remote` capability → `playback_event` from `https://music.youtube.com`): **works** (`window.__TAURI_INTERNALS__.invoke` from the init script).
Session shared between login, player and main windows: **yes** (same cookie list from every window).
Session survives app restart: **yes** (hidden playback after relaunch, without signing in again, played as the signed-in Premium user).

## What went wrong first, and why

- **Empty cookie dump.** `Webview::cookies_for_url("https://music.youtube.com/")` returned `[]`. wry's macOS implementation compares the cookie domain to the URL host for exact equality (`wry-0.57.3/src/wkwebview/mod.rs` `cookies_for_url`), so `.youtube.com` cookies never match `music.youtube.com`. Reading `Webview::cookies()` and filtering by domain suffix works.
- **No autoplay on first run.** YouTube Music's watch page loads the media (`loadedmetadata`) but stays paused. This is the page's behavior, not a WebView policy block: wry's default `autoplay: true` already sets `mediaTypesRequiringUserActionForPlayback = []` on macOS, and the Swift app also starts playback itself (`SingletonPlayerWebView+ObserverScript.swift`, autoplay recovery). Calling `video.play()` from the init script on `loadedmetadata` succeeded.

## Consequences for M2

- **Cookies:** read with `Webview::cookies()` and keep cookies whose domain (leading dot trimmed) is `youtube.com` or ends with `.youtube.com`; never `cookies_for_url`. Build `ytm_core::auth::CookieJar` from those (name, value) pairs. `__Secure-3PAPISID` is present, confirming the SAPISID preference fixed in `12fa4e6`.
- **Playback start:** the player bridge must call `video.play()` after load (with retry, like the Swift autoplay recovery) — the page will not start on its own.
- **Windows autoplay flag:** wry already appends `--autoplay-policy=no-user-gesture-required` on WebView2 when `autoplay` is true (the default). The spike's explicit `additional_browser_args` call is redundant and replaces Tauri's default browser args; drop it in the app.
- **DRM:** macOS works without standard EME. On Windows, WebView2 exposes standard EME; whether YouTube Music serves Widevine there and whether it plays is the main open question for hardware testing.
- **Security (from the Task 1 review):** the app's `playback_event` must take a typed, size-bounded payload and ignore events for a video other than the current one; validate `videoId` against `^[A-Za-z0-9_-]{11}$` before building watch URLs; set `withGlobalTauri: false`, a CSP for local windows, and a navigation allowlist for the login and player windows.
- **Region:** signed-out InnerTube requests in Korea return only "In your area, YouTube Music is only available to Premium members." Signed-out mode is not useful here; the app should go straight to sign-in.

## Still open

- All four risks on Windows (WebView2) — run this spike on Windows hardware, or the M2 app's CI-built installer.
- A real signed-in home response for parser fixtures (`ytm-explorer browse FEmusic_home` with a cookie file).
