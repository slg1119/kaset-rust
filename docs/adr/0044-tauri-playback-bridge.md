# ADR-0044: Playback bridge, queue, and OS media controls in the Tauri app

## Status

Accepted (2026-11-22, milestone M3 of the Tauri port, ADR-0042).

## Context

YouTube Music Premium audio is Widevine-protected, so the Tauri app plays it in
a WebView on music.youtube.com, as the Swift app does (ADR-0001). The M0 spike
(`docs/superpowers/spikes/2026-11-22-tauri-playback.md`) showed that a hidden
Tauri window plays signed-in audio on macOS when the host calls `video.play()`,
and that a capability with `remote.urls` lets the page call one command. M3 keeps
the MVP simpler than the Swift player: one full `watch?v=` load per song, with
the queue owned by Rust.

## Decision

- **Player window:** a hidden window labelled `player`, created on the first
  play and navigated to `https://music.youtube.com/watch?v=<id>` for every song.
  It shares the WebView cookie store with the login and main windows, uses the
  login window's Safari user agent on macOS, disables background throttling
  (macOS 14+), and adds no browser arguments (WebView2 windows share one
  environment, and different arguments fail). Sign-out closes it; closing the
  main window quits the app so hidden audio cannot outlive the UI.
- **Bridge:** an initialization script (main frame, `music.youtube.com` only)
  hooks the `<video>` element, starts playback with `video.play()` (up to five
  attempts), and reports `progress`, `ended`, and `failed`. Periodic progress
  is limited to 4 Hz, with immediate reports on playback, seek, and metadata
  events. It detects ads from the player classes and
  `getPresentingPlayerType(true) === 2`.
  After a pause command or an ended song it holds playback so the page's own
  autoplay cannot start another song. Rust drives it with `eval` of three fixed
  calls (`play`, `pause`, `seek(<number>)`).
- **IPC hardening:** capability `player-remote` grants only
  `allow-playback-event` to `https://music.youtube.com/*` in the `player`
  window. The command also checks the calling window label
  and URL. `PlaybackEvent` is a tagged enum with fixed fields and
  `deny_unknown_fields`; its video id is validated while deserializing. Events
  for any video other than the current song are ignored. Every video id is
  checked against `^[A-Za-z0-9_-]{11}$` before it reaches a URL, and songs with
  invalid ids are dropped from queues. At the command boundary, `play_queue`
  rejects queues above 500 songs or song and artist strings above 2048 bytes
  with `invalid_argument`. The player window has its own navigation
  allowlist (https on YouTube, Google, and Google media/ad hosts, country Google
  domains, `about:`/`blob:`/`data:` frames); blocked navigations log scheme and
  host only.
- **Queue and state:** Rust owns the queue (`player::queue`) and the player
  state machine (`player::engine`), both pure and unit-tested. Repeat is
  off/all/one; shuffle keeps the current song first and permutes the rest with
  a clock-seeded xorshift (no dependency). An ended song advances the queue;
  repeat one reloads it; the end of the queue holds the page in `ended`.
  Previous restarts the song after three seconds or when there is no previous
  song. The engine clamps times to one week (`MAX_MEDIA_SECONDS`).
  `player://state` reports status, track, and ad changes immediately and
  throttles steady progress to one event per 250 ms. `player://queue` reports
  queue or position changes.
- **OS media controls:** `souvlaki` 0.9.3. Its handle is created, updated, and
  dropped only on the main thread (`run_on_main_thread` plus a thread-local), as
  macOS `MPRemoteCommandCenter` and Windows SMTC (bound to the main window's
  HWND) require. Media events are handed to the async runtime before touching
  the player. Metadata is sent when it changes and cleared when the queue
  empties. Artwork is passed only for HTTPS URLs. Media positions and durations
  use `Duration::try_from_secs_f64`. Playback updates follow status changes,
  metadata changes, or position drift consistent with a seek. The OS advances
  a playing position itself.
- **Session hardening carried from M2 reviews:** login completion, API
  expiry, and launch and player cookie refresh use session epochs captured
  before their reads or requests (`sign_in_if`, `mark_expired_if`,
  `refresh_if`). The launch gate opens when refresh completes or times out
  after five seconds. WebView cookie reads and deletion, session expiry, and
  keyring I/O run on the blocking pool. `PlayerService::stop_for_sign_out`
  holds the player lock through cookie cleanup and session sign-out so playback
  cannot reopen the window during cleanup. Details are in ADR-0043.

## Consequences

### Positive

- The UI controls playback and the queue without handling cookies, and the
  remote page can only submit validated playback events.
- The queue and state machine are covered by plain `cargo test`.

### Negative

- Each song is a full page load: slower track changes than the Swift app's
  in-page router, and no gapless playback (planned later).
- The init script relies on undocumented YouTube Music page details (`<video>`,
  player classes, `getPresentingPlayerType`), which can change without notice.
- WebKit and WebView2 may publish their own media session for the page next to
  souvlaki's; the human checklist verifies which one the OS shows.
- A pause during `Loading` can be lost before the new document installs its
  bridge. A `Failed` event leaves the current track paused without auto-skipping.
- The navigation allowlist includes broad roots such as `google.com` and
  `googleusercontent.com`, and allows the `data:` scheme. Runtime checks are
  still needed for iframe IPC routing and `window.open` behavior.
- `souvlaki` pulls in `block` 0.1.6 on macOS, which triggers a future
  incompatibility warning in Rust.
