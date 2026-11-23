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
  environment, and different arguments fail). Closing the main window quits
  the app so hidden audio cannot outlive the UI.
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
  window. The command checks the calling window label and top-level URL as
  defense in depth and returns `invalid_argument` for a wrong caller.
  `PlaybackEvent` is a tagged enum with fixed fields and
  `deny_unknown_fields`; its video id is validated while deserializing. Events
  for any video other than the current song are ignored. Every video id is
  checked against `^[A-Za-z0-9_-]{11}$` before it reaches a URL, and songs with
  invalid ids are dropped from queues. At the command boundary, `play_queue`
  rejects queues above 500 songs or song and artist strings above 2048 bytes
  with `invalid_argument`. The player window has its own navigation
  allowlist (https on YouTube, Google, and Google media/ad hosts, country Google
  domains, `about:`/`blob:`/`data:` frames); blocked navigations log scheme and
  host only. In both the player and login windows, `window.open` is denied
  (explicit policy / wry default).
- **Queue and state:** Rust owns the queue (`player::queue`) and the player
  state machine (`player::engine`), both pure and unit-tested. Repeat is
  off/all/one; shuffle keeps the current song first and permutes the rest with
  a clock-seeded xorshift (no dependency). An ended song advances the queue;
  repeat one reloads it; the end of the queue holds the page in `ended`.
  Previous restarts the song after three seconds or when there is no previous
  song. The engine clamps times to one week (`MAX_MEDIA_SECONDS`).
  `player://state` reports status and track changes immediately. Bridge events
  that leave the status unchanged, including ad flag flips, are throttled to
  one event per 250 ms. `player://queue` reports queue or position changes.
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
  after five seconds. See ADR-0043 for blocking-pool and startup restore details.
- **Playback and auth transitions:** sign-out and `SignedIn → Expired` clear the
  queue, close the player window, and publish `Stopped` to OS media controls.
  Expired-jar recovery stops the player before clearing the shared WebView
  cookies. The player lock serializes playback commands with these transitions
  and cookie cleanup. Each login window permits one interactive retry after
  `SameAsExpired`. A second rejection closes the login window and keeps the
  session expired.

## Autoplay and notifications

With repeat off, a queue position change that leaves fewer than three upcoming
tracks triggers a signed-in radio request. The player seeds radio from the
last track in play order and uses the returned continuation for subsequent
pages. New tracks are appended in response order, including during shuffle.
The queue caps at 500 tracks by evicting earlier play history as needed.
One request runs at a time, and queue generation and session epoch checks
discard stale responses. If the current track ends while its radio request is
in flight, playback advances when the response supplies a playable track.

A playing track with a new video ID triggers a native notification once its
content plays after any ad. The notification uses the song title and
comma-separated artist names. Stopping clears the last notified ID so the
same track can be announced when played again. The service sends through the
Rust notification plugin after releasing the player lock.

The lyrics panel uses the current track in `player://state` to highlight
timed lines and center the active line, respecting reduced motion. Clicking
a line sends its timestamp in seconds through the same seek path as the
player slider. During ads, lyric highlighting and click-to-seek pause.

Podcast show rows use the same video-ID queue as music tracks. The UI maps
loaded episodes to songs with the show title as the artist and starts at the
selected episode, within the existing 500-track limit.

## M8 player session recovery

The first player window opens on `about:blank`. Each track load checks the
actual player store for a live YouTube Music auth cookie and restores the
signed-in session when that cookie is missing. The player lock serializes
this work with sign-in, sign-out, and API expiry. The blocking pool reads
and writes cookies under the current session epoch. Wry normalizes parent
scope on reads, so a parent-only result triggers a conservative restore
until the player can match the current epoch and confirmed restore expiry.
Signing cookies stay readable by YouTube Music's page JavaScript.

The native page-load callback associates off-site finishes with a load
generation. The engine pauses a matching pending track and emits one
`player://redirected` notice to the main window. The current queue position
stays available for a manual retry.

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
  `googleusercontent.com`, and allows the `data:` scheme. Iframe IPC is verified
  by source (ACL on request Origin, IPC injected into the main frame only).
  Confirm at runtime that third-party iframes get no IPC.
- `souvlaki` pulls in `block` 0.1.6 on macOS, which triggers a future
  incompatibility warning in Rust.

## M5b artist playback

The artist hero resolves a radio or shuffle endpoint through InnerTube and
starts the returned tracks through the existing queue engine. The engine keeps
the continuation for autoplay. A changed session or replaced queue prevents a
late response from starting playback. When there is no mix endpoint, the hero
queues the page’s top songs.

## M5b CLI playback

The running desktop app exposes a same-user local endpoint for CLI controls.
The CLI dispatch shares the player service and retains normal queue and
autoplay behavior. See [ADR-0046](0046-cli-ipc.md) for the protocol and threat
model.

## M5b playlist and album queues

Detail screens and the player bar share one snapshot and event subscription
through the signed-in shell. Detail playback associates each queued song with
its original browse row. Queue events identify that row through shuffle and
when a track occurs more than once. Starting a detail shuffle sets shuffle
and replaces the queue under the same player lock.

## M5b queue editing and song radio

Queue commands address indices in the displayed play order. Play Next moves
an upcoming occurrence, and Add to Queue skips it. Either action can replay
a current or played track that has no upcoming occurrence. The 500-song cap
can evict played history to make room. Reordering stays within upcoming tracks.
Removing the last current track ends playback and retains played history.
Adding after removal or natural completion appends after the history and
starts the first addition.
A fresh queue can hold staged songs until the user starts playback. Queue
additions report duplicate and capacity outcomes separately. Turning shuffle
off restores the underlying order of retained songs.

Start Radio replaces the queue with its seed song and requests the first radio
page through the existing InnerTube client. Autoplay schedules one request at
a time and discards results from an obsolete radio generation or session.
Enqueuing a track preserves the current radio continuation. Clear upcoming
invalidates pending radio results. Changing shuffle, switching repeat
off, and pressing Play or Next after the queue ends let the engine recheck
whether to request more songs.

## M5b track notification preference

Track-change notifications default to on. The app saves changes atomically in
`track-notifications.json` in its app data directory. The player checks the
setting before sending each OS notification and still tracks playback while
notifications are disabled.
