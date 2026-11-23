# Kaset Tauri Port — Design

- Date: 2026-11-22
- Status: Implemented through M6. See the verification items below.
- Branch: `main`

## Goal

Replace the original macOS app with a single Tauri 2 app that runs on
macOS and Windows, with the business logic in Rust and the UI in React.

## Decisions

| Topic | Decision |
|-------|----------|
| Relationship to original app | **Full replacement.** The original sources served as a porting reference and were removed at the MVP cutover (ADR-0047). |
| Repository | Same repository (`kaset-rust`). |
| Frontend | React + TypeScript + Vite, pnpm. |
| Logic placement | **Rust-centric.** API client, parsers, auth/cookies, queue and player state live in Rust. React renders and caches. |
| Dependencies | Tauri dependencies are evaluated on their own merits. |
| Test environment | Development on macOS. Windows verified through CI build artifacts now and on real hardware later. A YouTube Music Premium account is available for DRM checks. |

## Scope

**MVP (milestone 1):** sign-in, home, search, playback with player bar, queue,
playlist and album detail, media keys / OS media controls.

**Later milestones:** lyrics, library editing, artist pages, podcasts,
scrobbling, auto-update, YouTube (video) mode, gapless playback via YouTube
Music's in-page router and Up Next sync.

**Out of scope:** App Intents, Foundation Models command bar, Liquid Glass,
AppleScript, extensions.

## User CLI (`kaset`)

A user-facing CLI built on the same `ytm-core` as the GUI. It is separate from
`ytm-explorer`, which stays a developer tool for raw API probes.

- **Standalone commands** call `ytm-core` directly and need no running app:
  search, home, playlist/album/library listing. Output is human-readable by
  default, with `--json` for scripting.
- **Playback commands** (`play`, `pause`, `next`, `previous`, `queue`,
  `now-playing`) remote-control a running Kaset app over local IPC (a Unix
  domain socket on macOS, a named pipe on Windows). Playback itself still
  happens in the app's hidden WebView, because Widevine DRM requires a browser
  engine (ADR-0001); the CLI never extracts stream URLs.
- **Auth:** the CLI reads the session the app stored in the OS secure store
  (`keyring`). It does not run its own sign-in flow; if no session exists it
  tells the user to sign in through the app.
- The app exposes the IPC endpoint only to the current OS user. Commands are a
  fixed set mirroring the Tauri player commands; the endpoint never accepts
  arbitrary JS or URLs.

## Architecture

```
kaset/
├─ app/
│  ├─ src/            # React + TS + Vite — UI only
│  └─ src-tauri/      # Tauri shell: windows, commands, events, OS integration
├─ crates/
│  ├─ ytm-core/       # Pure Rust: InnerTube client, parsers, models (no Tauri dependency)
│  ├─ ytm-explorer/   # Read-only probes for verifying real responses
│  ├─ kaset-session/  # Shared OS secure store and debug file store for app and CLI
│  ├─ kaset-ipc/      # Local playback protocol and transport
│  └─ kaset-cli/      # User CLI: ytm-core + app remote control
└─ Cargo.toml         # Cargo workspace
```

### `crates/ytm-core`

- InnerTube client for `music.youtube.com/youtubei/v1` with client `WEB_REMIX`,
  SAPISIDHASH authorization computed from injected cookies.
- Endpoint-specific parser modules for home, search, details, and radio.
- serde models for songs, albums, playlists, artists, shelves.
- Does not store cookies; callers pass a cookie set in.
- Buildable and testable with `cargo test` alone.

### `app/src-tauri`

- Application state: session, queue, player state.
- Cookie persistence in the OS secure store via `keyring`
  (macOS Keychain, Windows Credential Manager).
- Playback WebView lifecycle and JS bridge.
- OS media integration via `souvlaki` (macOS Now Playing, Windows SMTC, media keys).
- Typed commands/events exported with `tauri-specta`.

### `app/src`

- Screens, routing, server-data caching with TanStack Query.
- Never sees cookies or auth headers.

## Data Flow

### Sign-in

1. A visible Tauri WebView opens `accounts.google.com`. Returning to
   `music.youtube.com` signals success.
2. Rust reads cookies (including `SAPISID`) through Tauri's WebView cookie API
   and stores them via `keyring`.
3. `ytm-core` uses them to sign requests. HTTP 401/403 → `AuthExpired` → UI
   prompts to sign in again.
4. On launch, cookies are restored from the secure store. The sign-in and
   playback WebViews share one WebView data directory, so they share a session.

### Playback

```
React ──invoke("play", videoId)──▶ Rust PlayerState
                                       │ eval(JS)
                                       ▼
                         Hidden WebView: music.youtube.com/watch?v=…
                         (init script: observes <video> events, detects ads)
                                       │ IPC: playback_event{state, progress, duration, videoId}
                                       ▼
Rust PlayerState ──emit("player://state")──▶ React player bar
                └──▶ souvlaki (Now Playing / SMTC / media keys)
```

- The remote origin `https://music.youtube.com` may call exactly one command,
  `playback_event`, granted through a Tauri capability with `remote.urls`.
- Track changes load a new `watch?v=` URL in the MVP. Router-based navigation
  and Up Next sync come later.
- Rust owns the queue and advances it on the track-ended event.

## Error Handling

- `ytm-core` defines `YtmError` with `thiserror`:
  `AuthExpired`, `Network`, `Http { status }`, `Parse { endpoint, path }`, `RateLimited`.
  `Parse` records the JSON path that failed.
- Parsers can skip malformed individual items and retain usable results.
- Tauri commands return `Result<T, AppError>`; `AppError` serializes as
  `{ kind, message }`.
- React: a global handler routes `AuthExpired` to sign-in; other errors show a
  per-screen error state with retry.

## IPC Contract

- Bindings generated by `tauri-specta` into `app/src/bindings.ts`. Hand-written
  `invoke("…")` strings are not allowed.
- Events: `player://state`, `player://queue`, `auth://changed`.
- Progress events are throttled to about 4 Hz.

## Secrets and Logging

- Logging with `tracing`.
- Cookies, SAPISID, and Authorization headers are never logged. They are held
  in a wrapper type whose `Debug` output is `REDACTED`.
- Test fixtures are scrubbed of account identifiers and tokens before commit.

## Testing

- **Parsers:** snapshot tests (`insta`) against real JSON fixtures in
  `crates/ytm-core/tests/fixtures/`, captured with `ytm-explorer` and scrubbed.
  Keep fixture payloads minimal.
- **API client:** fixed-input unit test for SAPISIDHASH; HTTP mocked with
  `wiremock`; 401/403 → `AuthExpired`.
- **Queue/player state machine:** Rust unit tests for next, previous, repeat,
  shuffle, and track-ended handling.
- **React:** Vitest + Testing Library for core components. E2E deferred.
- **Real playback:** manual checklist.

## CI

GitHub Actions matrix on macOS and Windows:

- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- `pnpm lint`, `pnpm typecheck`, `pnpm test`
- `tauri build`, uploading the Windows installer (`.msi` / NSIS) as an artifact
  for hardware verification.

Version tags publish a universal macOS DMG and an x64 Windows NSIS installer.

## Risks — verify first (spike, first implementation step)

1. Tauri's cookie API can read `HttpOnly` cookies such as `SAPISID`.
2. The hidden WebView can autoplay without a user gesture (WKWebView
   configuration on macOS; WebView2 `--autoplay-policy=no-user-gesture-required`
   on Windows).
3. Premium (Widevine) tracks play in Tauri's macOS WebView (checkable now) and
   in WebView2 (checked later on hardware). Public reports indicate WebView2
   inherits Edge's Widevine support, but nothing confirms YouTube Music
   specifically.
4. Google sign-in is not rejected inside the embedded WebView ("This browser
   or app may not be secure"). macOS sign-in uses WKWebView with a Safari user agent; WebView2 on Windows is unverified.

If any of these fails, stop and revisit the design before building further.

## Documentation

- ADR `docs/adr/0042-tauri-rust-rewrite.md` records this decision.
- `AGENTS.md` documents the Tauri workspace and its contribution rules.
- ADR-0047 records the removal of the original app and release path.

## Milestones

| Milestone | Delivered | Status |
| --- | --- | --- |
| M0 | Playback and sign-in spike | Done |
| M1 | Workspace, InnerTube client, and API explorer | Done |
| M2 | Sign-in and session persistence | Done |
| M3 | Playback bridge, queue, and OS media controls | Done |
| M4 | UI shell, home, search, and player bar | Done |
| M5 | Playlist and album detail, M5b feature work, and MVP cutover | Done |
| M6 | Standalone CLI and playback remote control | Done |

## Verification status

- Windows hardware verification remains open for sign-in, DRM playback,
  media keys, and native notifications.
- Live signed-in API verification remains open for the parity wave.
- The track-change notification toggle shipped in M5b.
