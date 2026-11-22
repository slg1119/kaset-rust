# Kaset Tauri Port — M3 (Playback Bridge, Queue, OS Media Controls) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Play YouTube Music songs from the Tauri app: a hidden player WebView on music.youtube.com driven by a Rust-owned queue, `player://state` / `player://queue` events, OS media controls (macOS Now Playing, Windows SMTC, media keys), and a minimal React player bar fed by the home screen's songs — after closing the session races and store weaknesses deferred from the M2 reviews.

**Architecture:** Tasks 1–2 harden the M2 session: two-slot atomic keyring saves, a session epoch (`sign_in_if` / `mark_expired_if`), rejection of the jar that expired, a launch gate for API commands, and WebView-cookie/keyring work moved to Tauri's blocking pool. Task 3 adds a pure-Rust queue and player state machine (`player::queue`, `player::engine`) that turns commands and page events into an `Outcome`. Task 4 adds the Tauri glue: a hidden `player` window with an init script and navigation allowlist, a `player-remote` capability that grants music.youtube.com only the typed `playback_event` command, a `PlayerService` that applies outcomes, and nine commands plus two events. Task 5 publishes state to `souvlaki` on the main thread and routes media keys back. Task 6 adds the React player bar and makes home songs playable. Task 7 records the decisions.

**Tech Stack (versions verified 2026-11-22 against crates.io, the crate sources in `~/.cargo/registry`, and the lockfile):**
- Rust (existing): tauri **2.12.7**, tauri-build **2.7.6**, wry 0.57.3, tauri-runtime-wry 2.12.5, tauri-utils 2.10.4, tauri-specta **=2.0.0-rc.29**, specta **=2.0.0-rc.28**, specta-typescript **=0.0.15**, keyring **4.2**, tokio **1.54.1**, windows **0.62.2** (transitive; `HWND(pub *mut c_void)`).
- Rust (new): **souvlaki 0.9.3** (MIT, rust-version 1.87, edition 2024; default feature `use_dbus` only affects Linux). tokio features gain `time` (the app) and `macros`, `rt` (dev). No other new crates (shuffle uses an in-crate xorshift).
- Frontend (existing, unchanged): @tauri-apps/api ~2.12.5, react 19, vitest 5, @testing-library/react 16, typescript ~5.9.3.

**Spec:** `docs/superpowers/specs/2026-11-22-tauri-port-design.md` (milestone 3: "Playback bridge, queue, OS media controls"). Read with it: spike findings `docs/superpowers/spikes/2026-11-22-tauri-playback.md`, ADR-0042, ADR-0043, and the M2 plan `docs/superpowers/plans/2026-11-22-tauri-port-m2.md` for the existing code's conventions.

## Global Constraints

- No real credentials anywhere — code, comments, logs, fixtures, docs, output. Placeholders only: `test-sapisid`, `test-cookie`. Video ids in tests are placeholders such as `test-vid-0A`.
- Never print or log cookie values, SAPISID, or Authorization headers; never format a `keyring::Error` (use `store::describe`). Log only the scheme and host of a blocked URL.
- `crates/ytm-core` must not depend on Tauri.
- Tauri **2.x only**. Commands and events reach the frontend only through `tauri-specta` bindings in `app/src/bindings.ts`; no hand-written `invoke("…")` in `app/src`. Regenerate with `cargo test -p kaset bindings_are_current` and commit the file in the same task.
- Call `Webview::cookies()` / `delete_cookie()` and keyring I/O only on the blocking pool (`crate::webview_session::blocking`), never on the main thread (deadlocks on Windows) or directly in async code.
- Touch `souvlaki::MediaControls` only on the main thread (`AppHandle::run_on_main_thread`).
- `withGlobalTauri: false` stays. The `player-remote` capability grants `allow-playback-event` and nothing else (no `core:*`). The login window keeps no capability.
- Every video id is validated with `VideoId::parse` (`^[A-Za-z0-9_-]{11}$`) before it reaches a URL or the engine. `eval` scripts never interpolate strings — only the fixed scripts and a sanitized `f64`.
- Do not pass `additional_browser_args` to any window: WebView2 windows share one environment and different arguments make window creation fail.
- Live requests to YouTube are read-only. Codex workers do not launch the GUI app (it would take over the human's desktop and needs a Google sign-in); runtime checks are in the human checklist at the end.
- Do not modify or delete Swift sources (`Sources/`, `Tests/`, `Package.swift`) or `spikes/`. Do not run Swift UI tests.
- Rust edition 2024. For touched crates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` must pass. For `app/`: `pnpm lint`, `pnpm typecheck`, `pnpm test` must pass.
- Commit directly on `main` and `git push origin main` at the end of every task (human directive: direct commits, no PRs). Stage only the files the task lists.

## Decisions made in this plan

Recorded in ADR-0044 and the amended ADR-0043 by Task 7.

1. **Atomic keyring saves:** two slots `session.a.N` / `session.b.N` plus a commit entry `session.current` = `a:<count>` or `b:<count>`, written after the chunks; the other slot is removed afterwards. The M2 layout (`session.count`, `session.N`) is read and migrated by the next save. Loads reject counts above `MAX_SWEEP_CHUNKS` (64); `clear` caps recorded counts at 64, deletes highest index first and stops at the first failure, and deletes `session.current` / `session.count` last.
2. **Session epoch:** `u64`, incremented by `restore`, `sign_in_if`, and `sign_out`; unchanged by `refresh` and `mark_expired_if`. Login captures it in `on_page_load` before reading cookies; `get_home` captures it before its request.
3. **Expired jar:** `mark_expired_if` remembers the jar in use. `sign_in_if` with the same auth-cookie set (`AUTH_COOKIE_NAMES`) fails with `SessionError::SameAsExpired`; the login window stays open. Cleared by a new epoch.
4. **Launch gate:** `get_home` waits up to 5 s (`LAUNCH_REFRESH_TIMEOUT`) for the launch cookie refresh, then proceeds (logging a warning on timeout).
5. **Refresh:** runs at launch and after every player page load on music.youtube.com, through `refresh_session_from_webview(app, label, reason)` on the blocking pool.
6. **Player window:** label `player`, hidden, created on first play, one full `watch?v=` navigation per song (spec), Safari UA on macOS (shared with login), `BackgroundThrottlingPolicy::Disabled`, closed by sign-out and by `WebCommand::Stop`. The app exits when the `main` window is destroyed, so hidden audio cannot outlive the UI.
7. **Player navigation allowlist:** https on `youtube.com`, `google.com`, `gstatic.com`, `googlevideo.com`, `ytimg.com`, `ggpht.com`, `googleusercontent.com`, `youtube-nocookie.com`, `doubleclick.net`, `googlesyndication.com` and subdomains, country Google domains, plus `about:`/`blob:`/`data:`. Wider than login because the page loads media, rotation, and ad frames; only music.youtube.com has IPC.
8. **`playback_event`:** `PlaybackEvent` = `progress { video_id, playing, position, duration, ad } | ended { video_id } | failed { video_id }`, `#[serde(tag = "kind", deny_unknown_fields)]`, `VideoId` validated during deserialization. The command also checks the webview label and URL. Events for a video other than the current song are ignored. The command is in the generated bindings (tauri-specta), but the `main` capability does not grant it.
9. **Queue:** Rust owns it. Repeat off/all/one (one = replay on end; next/previous still move). Shuffle keeps the current song first and permutes the rest (xorshift seeded from the clock). End of queue → status `ended` and the page is held; Play then reloads the song. Previous restarts after 3 s or at the start of the queue. A queue holds at most 500 songs.
10. **Init script:** `video.play()` up to 5 attempts 500 ms apart, then `failed`; `hold` flag after pause or ended blocks the page's own autoplay; minimal ad detection (`ad-showing`/`ad-interrupting`, `getPresentingPlayerType(true) === 2`); `ended` not sent during ads; progress at most every 250 ms. Rust throttles progress-only `player://state` to 250 ms (~4 Hz) and emits status/track/ad changes immediately.
11. **Events:** `player://state` (`PlayerStateChanged { state }`) and `player://queue` (`QueueChanged { queue }`), emitted with `emit_to("main")`. tauri-specta names the TS accessors after the event name: `events.playerState`, `events.playerQueue`. `f64` fields are typed `number | null` in TS.
12. **Media controls:** `souvlaki` 0.9.3 in a main-thread `thread_local`; Windows uses the `main` window's HWND; media events are mapped (`MediaAction`) and applied on the async runtime. Metadata is sent on track change, playback state on status change or a >1.5 s jump.
13. **Errors:** new `ErrorKind::InvalidArgument` (`invalid_argument`) for rejected queues.
14. **UI:** the player bar is hidden while idle; home shelves list their songs; clicking a song calls `playQueue(shelfSongs, index)`. Seeking commits on slider release. TanStack Query stays deferred to M4.

## Items deferred from the M2 reviews

From `.superpowers/sdd/2026-11-22-tauri-port-m2/progress.md` ("Ruling:" lines):

| Deferred item | Task |
|---|---|
| Session epoch: login completion vs sign-out, stale `get_home` 401 expiring a newer session, launch race (401 before the launch refresh); barrier tests | 2 |
| Expired re-sign-in loop (identical jar) | 2 |
| `cookies()`/keyring work via `spawn_blocking`; refresh log/ADR no longer "launch only" | 2, 4, 7 |
| Player window hardening: `player` label, remote capability with only `playback_event`, typed bounded payload, ignore other videoIds, videoId validation, navigation allowlist, no `withGlobalTauri` | 3, 4 |
| Atomic generation-based keyring saves; cap a numeric corrupt count at `MAX_SWEEP_CHUNKS` | 1 |
| `refresh` while `Expired` is a no-op (M2 Task 5 minor) | Kept: only a new sign-in recovers; noted in ADR-0043 |

## File Structure

```
crates/ytm-core/src/models.rs                # Song, ArtistRef derive Deserialize (+ round-trip test)
app/src-tauri/
  Cargo.toml                                 # tokio "time"; dev tokio "macros","rt"; souvlaki 0.9.3
  build.rs                                   # app manifest lists the 9 player commands
  capabilities/default.json                  # main: + 8 player UI commands (not playback_event)
  capabilities/player.json                   # NEW player-remote: music.youtube.com → allow-playback-event only
  permissions/autogenerated/*.toml           # 9 new files generated by tauri-build (committed)
  src/lib.rs                                 # modules, commands/events, media attach, exit on main destroyed
  src/error.rs                               # + ErrorKind::InvalidArgument
  src/events.rs                              # + PlayerStateChanged, QueueChanged
  src/state.rs                               # + LaunchGate, AppState { launch, player }
  src/webview_session.rs                     # NEW blocking(), refresh_session_from_webview()
  src/login.rs                               # epoch capture, blocking sign-in completion
  src/commands.rs                            # sign_out (stop player, blocking), get_home (gate, epoch)
  src/session/mod.rs                         # epoch, sign_in_if, mark_expired_if, expired jar
  src/session/store.rs                       # two-slot ChunkedStore
  src/session/cookies.rs                     # is_under → pub(crate)
  src/player/mod.rs                          # NEW
  src/player/video_id.rs                     # NEW VideoId
  src/player/queue.rs                        # NEW Queue, RepeatMode, Advance
  src/player/engine.rs                       # NEW Player, PlayerState, QueueState, PlaybackEvent, Outcome
  src/player/player.js                       # NEW init script
  src/player/window.rs                       # NEW hidden window, allowlist, scripts
  src/player/service.rs                      # NEW PlayerService (applies outcomes, media publish)
  src/player/commands.rs                     # NEW 9 commands
  src/player/media.rs                        # NEW souvlaki glue
app/src/
  bindings.ts                                # regenerated (Tasks 2, 4)
  App.tsx, App.test.tsx                      # PlayerBar when signed in; play-from-home tests
  home/HomeScreen.tsx                        # songs per shelf, click → playQueue
  player/format.ts, usePlayer.ts, PlayerBar.tsx, PlayerBar.test.tsx, testing.ts   # NEW
  styles.css                                 # list and player bar styles
docs/adr/0044-tauri-playback-bridge.md       # NEW
docs/adr/0043-tauri-app-session-and-ipc.md   # amended
AGENTS.md                                    # Tauri section
```

Task order: 1 → 2 → 3 → 4 → 5 → 6 → 7. Task 2 depends on Task 1 only by file proximity; Task 4 depends on Tasks 2 and 3; Task 5 on Task 4; Task 6 on Task 4's bindings.

---

### Task 1: Session store — two-slot atomic saves and capped counts

The M2 `ChunkedStore` overwrites `session.0..N` in place, so a save interrupted mid-way mixes old and new chunks, and `clear` trusts a numeric `session.count` of any size. This task writes each save into the slot that is not committed and commits with a single entry write.

**Files:**
- Modify: `app/src-tauri/src/session/store.rs`
- Test: `app/src-tauri/src/session/store.rs` (tests module)

**Interfaces:**
- Consumes: existing `SessionStore` (`load`/`save`/`clear`), `EntryBackend` (`get`/`set`/`delete`), `StoreError`, `KeyringBackend`, `FileStore`, `MemoryStore`, `store_kind`, `store_for` — all unchanged.
- Produces: same public API. New private items: `CURRENT_ENTRY = "session.current"`, `LEGACY_COUNT_ENTRY = "session.count"`, `MAX_SWEEP_CHUNKS = 64`, `enum Slot { A, B, Legacy }` with `Slot::chunk(index) -> String` (`session.a.N`, `session.b.N`, `session.N`). Keyring layout after a save: `session.current` + one slot's chunks. `StoreError::Corrupt("current")` for an unreadable or too-large commit entry, `Corrupt("count")` for a legacy count above 64, `Corrupt("header too long")` for a header above 64 chunks.

- [ ] **Step 1: Write the failing tests**

In `app/src-tauri/src/session/store.rs`, replace everything from the line `#[cfg(test)]` that is immediately followed by `mod tests {` down to the end of the file with:

```rust
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct MapBackend {
        entries: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        fail_delete_once: Arc<Mutex<Option<&'static str>>>,
        fail_set_once: Arc<Mutex<Option<&'static str>>>,
        calls: Arc<AtomicUsize>,
    }

    impl MapBackend {
        fn names(&self) -> Vec<String> {
            self.entries.lock().unwrap().keys().cloned().collect()
        }

        fn fail_next_delete_of(&self, name: &'static str) {
            *self.fail_delete_once.lock().unwrap() = Some(name);
        }

        fn fail_next_set_of(&self, name: &'static str) {
            *self.fail_set_once.lock().unwrap() = Some(name);
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl EntryBackend for MapBackend {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.entries.lock().unwrap().get(name).cloned())
        }
        fn set(&self, name: &str, value: &[u8]) -> Result<(), StoreError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let mut fail_set = self.fail_set_once.lock().unwrap();
            if *fail_set == Some(name) {
                *fail_set = None;
                return Err(StoreError::Keyring("platform failure"));
            }
            self.entries
                .lock()
                .unwrap()
                .insert(name.into(), value.to_vec());
            Ok(())
        }
        fn delete(&self, name: &str) -> Result<(), StoreError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let mut fail_delete = self.fail_delete_once.lock().unwrap();
            if *fail_delete == Some(name) {
                *fail_delete = None;
                return Err(StoreError::Keyring("platform failure"));
            }
            self.entries.lock().unwrap().remove(name);
            Ok(())
        }
    }

    fn long_header(len: usize) -> Secret {
        let value = "x".repeat(len - "SAPISID=".len());
        Secret::new(format!("SAPISID={value}"))
    }

    fn short_header() -> Secret {
        Secret::new("SAPISID=test-sapisid")
    }

    #[test]
    fn chunked_store_round_trips_a_short_header() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&short_header()).unwrap();
        assert_eq!(store.load().unwrap(), Some(short_header()));
        assert_eq!(backend.names(), ["session.a.0", "session.current"]);
    }

    #[test]
    fn chunked_store_splits_long_headers() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        let header = long_header(CHUNK_SIZE * 2 + 10);
        store.save(&header).unwrap();
        assert_eq!(store.load().unwrap(), Some(header));
        assert_eq!(
            backend.names(),
            ["session.a.0", "session.a.1", "session.a.2", "session.current"]
        );
    }

    #[test]
    fn chunked_store_alternates_slots_and_removes_the_previous_copy() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&short_header()).unwrap();
        let long = long_header(CHUNK_SIZE * 3);
        store.save(&long).unwrap();
        assert_eq!(store.load().unwrap(), Some(long));
        assert_eq!(
            backend.names(),
            ["session.b.0", "session.b.1", "session.b.2", "session.current"]
        );
        store.save(&short_header()).unwrap();
        assert_eq!(store.load().unwrap(), Some(short_header()));
        assert_eq!(backend.names(), ["session.a.0", "session.current"]);
    }

    #[test]
    fn chunked_store_interrupted_save_keeps_the_previous_session() {
        for failing in ["session.b.0", "session.b.1", "session.current"] {
            let backend = MapBackend::default();
            let store = ChunkedStore::new(backend.clone());
            store.save(&short_header()).unwrap();
            backend.fail_next_set_of(failing);
            assert!(store.save(&long_header(CHUNK_SIZE * 2)).is_err());
            assert_eq!(store.load().unwrap(), Some(short_header()), "{failing}");
        }
    }

    #[test]
    fn chunked_store_reads_and_migrates_the_legacy_layout() {
        let backend = MapBackend::default();
        backend.set("session.count", b"2").unwrap();
        backend.set("session.0", b"SAPISID=").unwrap();
        backend.set("session.1", b"test-sapisid").unwrap();
        let store = ChunkedStore::new(backend.clone());
        assert_eq!(store.load().unwrap(), Some(short_header()));

        let header = Secret::new("SAPISID=test-sapisid-2");
        store.save(&header).unwrap();
        assert_eq!(store.load().unwrap(), Some(header));
        assert_eq!(backend.names(), ["session.a.0", "session.current"]);
    }

    #[test]
    fn chunked_store_load_is_none_when_empty() {
        let store = ChunkedStore::new(MapBackend::default());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn chunked_store_rejects_missing_chunks() {
        let backend = MapBackend::default();
        backend.set(CURRENT_ENTRY, b"a:2").unwrap();
        backend.set("session.a.0", b"SAPISID=").unwrap();
        let store = ChunkedStore::new(backend);
        assert!(matches!(
            store.load(),
            Err(StoreError::Corrupt("missing chunk"))
        ));
    }

    #[test]
    fn chunked_store_rejects_counts_above_the_cap() {
        let backend = MapBackend::default();
        backend.set(CURRENT_ENTRY, b"a:65").unwrap();
        let store = ChunkedStore::new(backend.clone());
        assert!(matches!(store.load(), Err(StoreError::Corrupt("current"))));

        backend.delete(CURRENT_ENTRY).unwrap();
        backend.set(LEGACY_COUNT_ENTRY, b"65").unwrap();
        assert!(matches!(store.load(), Err(StoreError::Corrupt("count"))));
    }

    #[test]
    fn chunked_store_save_with_a_corrupt_current_entry_writes_slot_a() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        backend.set(CURRENT_ENTRY, b"invalid").unwrap();
        store.save(&short_header()).unwrap();
        assert_eq!(store.load().unwrap(), Some(short_header()));
        assert_eq!(backend.names(), ["session.a.0", "session.current"]);
    }

    #[test]
    fn chunked_store_save_succeeds_and_later_heals_when_stale_deletion_fails() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        backend.fail_next_delete_of("session.a.2");
        store.save(&short_header()).unwrap();
        assert_eq!(store.load().unwrap(), Some(short_header()));
        assert_eq!(
            backend.names(),
            [
                "session.a.0",
                "session.a.1",
                "session.a.2",
                "session.b.0",
                "session.current"
            ]
        );

        let header = Secret::new("SAPISID=test-sapisid-2");
        store.save(&header).unwrap();
        assert_eq!(store.load().unwrap(), Some(header));
        assert_eq!(backend.names(), ["session.a.0", "session.current"]);
    }

    #[test]
    fn chunked_store_clear_removes_every_slot() {
        let backend = MapBackend::default();
        backend.set("session.count", b"1").unwrap();
        backend.set("session.0", b"SAPISID=test-sapisid").unwrap();
        backend.set("session.b.0", b"test-cookie").unwrap();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE + 1)).unwrap();
        backend.set("session.b.0", b"test-cookie").unwrap();
        store.clear().unwrap();
        assert!(backend.names().is_empty());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn chunked_store_clear_sweeps_beyond_recorded_and_corrupt_counts() {
        for current in [b"a:2".as_slice(), b"invalid".as_slice()] {
            let backend = MapBackend::default();
            backend.set(CURRENT_ENTRY, current).unwrap();
            for index in 0..4 {
                backend.set(&Slot::A.chunk(index), b"test-cookie").unwrap();
            }
            backend.set(LEGACY_COUNT_ENTRY, b"invalid").unwrap();
            backend.set("session.0", b"test-cookie").unwrap();
            ChunkedStore::new(backend.clone()).clear().unwrap();
            assert!(backend.names().is_empty());
        }
    }

    #[test]
    fn chunked_store_clear_caps_huge_recorded_counts() {
        for (name, value) in [
            (CURRENT_ENTRY, b"a:1000000000".as_slice()),
            (LEGACY_COUNT_ENTRY, b"1000000000".as_slice()),
        ] {
            let backend = MapBackend::default();
            backend.set(name, value).unwrap();
            backend.set("session.a.0", b"test-cookie").unwrap();
            backend.set("session.0", b"test-cookie").unwrap();
            let before = backend.calls();
            ChunkedStore::new(backend.clone()).clear().unwrap();
            assert!(backend.names().is_empty());
            assert!(
                backend.calls() - before <= 3 * (2 * MAX_SWEEP_CHUNKS) + 4,
                "clear must stay bounded"
            );
        }
    }

    #[test]
    fn chunked_store_clear_can_retry_failed_chunk_deletions() {
        type Setup = fn(&MapBackend);
        let cases: [(&str, Setup, &'static str); 4] = [
            (
                "committed",
                |backend| {
                    ChunkedStore::new(backend.clone())
                        .save(&long_header(CHUNK_SIZE * 3))
                        .unwrap();
                },
                "session.a.1",
            ),
            (
                "corrupt current",
                |backend| {
                    ChunkedStore::new(backend.clone())
                        .save(&long_header(CHUNK_SIZE * 3))
                        .unwrap();
                    backend.set(CURRENT_ENTRY, b"invalid").unwrap();
                },
                "session.a.1",
            ),
            (
                "legacy",
                |backend| {
                    backend.set(LEGACY_COUNT_ENTRY, b"3").unwrap();
                    for index in 0..3 {
                        backend.set(&Slot::Legacy.chunk(index), b"test-cookie").unwrap();
                    }
                },
                "session.1",
            ),
            (
                "beyond recorded",
                |backend| {
                    backend.set(CURRENT_ENTRY, b"a:2").unwrap();
                    for index in 0..4 {
                        backend.set(&Slot::A.chunk(index), b"test-cookie").unwrap();
                    }
                },
                "session.a.3",
            ),
        ];
        for (case, setup, failing) in cases {
            let backend = MapBackend::default();
            setup(&backend);
            backend.fail_next_delete_of(failing);
            let store = ChunkedStore::new(backend.clone());
            assert!(store.clear().is_err(), "{case}");
            assert!(backend.names().contains(&failing.to_owned()), "{case}");
            store.clear().unwrap();
            assert!(backend.names().is_empty(), "{case}: {:?}", backend.names());
        }
    }

    #[test]
    fn file_store_round_trips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("nested").join(DEV_FILE_NAME));
        assert_eq!(store.load().unwrap(), None);
        store.save(&Secret::new("SAPISID=test-sapisid")).unwrap();
        assert_eq!(
            store.load().unwrap(),
            Some(Secret::new("SAPISID=test-sapisid"))
        );
        store.clear().unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn file_store_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DEV_FILE_NAME);
        FileStore::new(path.clone())
            .save(&Secret::new("SAPISID=test-sapisid"))
            .unwrap();
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn file_store_tightens_permissions_on_existing_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DEV_FILE_NAME);
        fs::write(&path, "SAPISID=old-test-sapisid").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let store = FileStore::new(path.clone());
        let header = Secret::new("SAPISID=test-sapisid");
        store.save(&header).unwrap();
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(store.load().unwrap(), Some(header));
    }

    #[test]
    fn file_store_is_debug_only() {
        assert_eq!(store_kind(Some("file"), true), StoreKind::File);
        assert_eq!(store_kind(Some("file"), false), StoreKind::Keyring);
        assert_eq!(store_kind(Some("keyring"), true), StoreKind::Keyring);
        assert_eq!(store_kind(None, true), StoreKind::Keyring);
    }

    #[test]
    fn store_errors_do_not_echo_secrets() {
        let error = keyring::Error::BadEncoding(b"SAPISID=test-sapisid".to_vec());
        let message = StoreError::Keyring(describe(&error)).to_string();
        assert_eq!(message, "secure store unavailable: bad encoding");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset session::store`
Expected: compile errors such as `cannot find value 'CURRENT_ENTRY' in this scope` and `failed to resolve: use of undeclared type 'Slot'`.

- [ ] **Step 3: Implement the two-slot store**

Replace everything above that `#[cfg(test)]` / `mod tests {` line (the whole non-test part of the file, including the `#[cfg(test)] MemoryStore` items, which are unchanged) with:

```rust
//! Persistent storage for the session's `Cookie` header.
//!
//! Release builds always use the OS secure store (macOS Keychain, Windows
//! Credential Manager) through `keyring`. Debug builds can opt into a plain
//! file with `KASET_SESSION_STORE=file`, for unsigned dev builds where the
//! Keychain prompts on every rebuild.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ytm_core::secret::Secret;

/// Keyring service name. Matches the bundle identifier in `tauri.conf.json`.
pub const KEYRING_SERVICE: &str = "com.sertacozercan.kaset.desktop";
/// Selects the session store in debug builds: `keyring` (default) or `file`.
pub const STORE_ENV: &str = "KASET_SESSION_STORE";
/// File name of the debug-only file store inside the app data directory.
pub const DEV_FILE_NAME: &str = "dev-session.txt";
/// Bytes per keyring entry. Windows Credential Manager caps a credential blob at
/// 2560 bytes and a signed-in youtube.com cookie header is usually larger.
pub const CHUNK_SIZE: usize = 2000;

/// Entry that commits a save: `<slot>:<count>`, for example `b:3`.
const CURRENT_ENTRY: &str = "session.current";
/// Chunk count of the M2 layout (`session.0..N`), read until the next save.
const LEGACY_COUNT_ENTRY: &str = "session.count";
/// Upper bound on chunks per slot, for loads, sweeps, and recorded counts.
const MAX_SWEEP_CHUNKS: usize = 64;

/// Where a header's chunks live. Saves alternate between `A` and `B`, so the
/// committed copy is never overwritten in place. `Legacy` is the M2 layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    A,
    B,
    Legacy,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::A, Slot::B, Slot::Legacy];

    fn chunk(self, index: usize) -> String {
        match self {
            Slot::A => format!("session.a.{index}"),
            Slot::B => format!("session.b.{index}"),
            Slot::Legacy => format!("session.{index}"),
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Slot::A => "a",
            Slot::B => "b",
            Slot::Legacy => "legacy",
        }
    }
}

/// Parses a decimal chunk count without applying the upper bound.
fn parse_count(raw: &[u8]) -> Option<usize> {
    std::str::from_utf8(raw).ok()?.parse().ok()
}

/// Parses `session.current` without applying the upper bound.
fn parse_current(raw: &[u8]) -> Option<(Slot, usize)> {
    let text = std::str::from_utf8(raw).ok()?;
    let (tag, count) = text.split_once(':')?;
    let slot = match tag {
        "a" => Slot::A,
        "b" => Slot::B,
        _ => return None,
    };
    Some((slot, count.parse().ok()?))
}

/// Errors never carry secret material: keyring errors are reduced to a fixed
/// description because some `keyring::Error` variants hold the raw secret bytes.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("secure store unavailable: {0}")]
    Keyring(&'static str),
    #[error("session file error: {0}")]
    Io(#[from] io::Error),
    #[error("stored session is corrupt: {0}")]
    Corrupt(&'static str),
}

/// Where the session's `Cookie` header is kept between launches.
pub trait SessionStore: Send + Sync {
    fn load(&self) -> Result<Option<Secret>, StoreError>;
    fn save(&self, header: &Secret) -> Result<(), StoreError>;
    fn clear(&self) -> Result<(), StoreError>;
}

/// Named binary entries. `KeyringBackend` implements it on the OS store;
/// tests implement it on a map.
pub trait EntryBackend: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError>;
    fn set(&self, name: &str, value: &[u8]) -> Result<(), StoreError>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, name: &str) -> Result<(), StoreError>;
}

/// Splits the header into 2000-byte chunks. A save writes the chunks into the
/// slot that is not committed, then commits by rewriting `session.current`, so
/// an interrupted save leaves the previous session readable.
pub struct ChunkedStore<B: EntryBackend> {
    backend: B,
}

impl<B: EntryBackend> ChunkedStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// The committed slot and chunk count, falling back to the M2 layout.
    fn current(&self) -> Result<Option<(Slot, usize)>, StoreError> {
        if let Some(raw) = self.backend.get(CURRENT_ENTRY)? {
            return parse_current(&raw)
                .filter(|(_, count)| *count <= MAX_SWEEP_CHUNKS)
                .map(Some)
                .ok_or(StoreError::Corrupt("current"));
        }
        match self.backend.get(LEGACY_COUNT_ENTRY)? {
            Some(raw) => parse_count(&raw)
                .filter(|count| *count <= MAX_SWEEP_CHUNKS)
                .map(|count| Some((Slot::Legacy, count)))
                .ok_or(StoreError::Corrupt("count")),
            None => Ok(None),
        }
    }

    /// The chunk counts recorded for each slot, capped at `MAX_SWEEP_CHUNKS`.
    /// Unreadable records count as zero.
    fn recorded_counts(&self) -> Result<[(Slot, usize); 3], StoreError> {
        let current = self
            .backend
            .get(CURRENT_ENTRY)?
            .and_then(|raw| parse_current(&raw));
        let legacy = self
            .backend
            .get(LEGACY_COUNT_ENTRY)?
            .and_then(|raw| parse_count(&raw))
            .unwrap_or(0);
        Ok(Slot::ALL.map(|slot| {
            let count = match (slot, current) {
                (Slot::Legacy, _) => legacy,
                (slot, Some((committed, count))) if slot == committed => count,
                _ => 0,
            };
            (slot, count.min(MAX_SWEEP_CHUNKS))
        }))
    }

    /// The first missing chunk at or after `start`, at most `MAX_SWEEP_CHUNKS`.
    fn sweep_end(&self, slot: Slot, start: usize) -> Result<usize, StoreError> {
        for index in start..MAX_SWEEP_CHUNKS {
            if self.backend.get(&slot.chunk(index))?.is_none() {
                return Ok(index);
            }
        }
        Ok(start.max(MAX_SWEEP_CHUNKS))
    }

    /// Deletes chunks from the highest index down and stops at the first
    /// failure, so the chunks left behind stay contiguous for a later sweep.
    fn delete_range(&self, slot: Slot, range: std::ops::Range<usize>) -> Result<(), StoreError> {
        for index in range.rev() {
            self.backend.delete(&slot.chunk(index))?;
        }
        Ok(())
    }

    /// Deletes every chunk except the first `count` chunks of `keep`, and the
    /// legacy count entry.
    fn remove_stale(&self, keep: Slot, count: usize) -> Result<(), StoreError> {
        let mut first_error = None;
        for slot in Slot::ALL {
            let start = if slot == keep { count } else { 0 };
            let result = self
                .sweep_end(slot, start)
                .and_then(|end| self.delete_range(slot, start..end));
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.backend.delete(LEGACY_COUNT_ENTRY) {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<B: EntryBackend> SessionStore for ChunkedStore<B> {
    fn load(&self) -> Result<Option<Secret>, StoreError> {
        let Some((slot, count)) = self.current()? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        for index in 0..count {
            let chunk = self
                .backend
                .get(&slot.chunk(index))?
                .ok_or(StoreError::Corrupt("missing chunk"))?;
            bytes.extend_from_slice(&chunk);
        }
        let header = String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("encoding"))?;
        Ok(Some(Secret::new(header)))
    }

    fn save(&self, header: &Secret) -> Result<(), StoreError> {
        let chunks: Vec<&[u8]> = header.expose().as_bytes().chunks(CHUNK_SIZE).collect();
        if chunks.len() > MAX_SWEEP_CHUNKS {
            return Err(StoreError::Corrupt("header too long"));
        }
        let committed = match self.current() {
            Ok(current) => current.map(|(slot, _)| slot),
            Err(StoreError::Corrupt(_)) => None,
            Err(error) => return Err(error),
        };
        let target = if committed == Some(Slot::A) {
            Slot::B
        } else {
            Slot::A
        };
        for (index, chunk) in chunks.iter().enumerate() {
            self.backend.set(&target.chunk(index), chunk)?;
        }
        let pointer = format!("{}:{}", target.tag(), chunks.len());
        self.backend.set(CURRENT_ENTRY, pointer.as_bytes())?;
        if self.remove_stale(target, chunks.len()).is_err() {
            tracing::warn!("Failed to remove stale session chunks");
        }
        Ok(())
    }

    /// Deletes all chunks before the entries that record them, so a failed
    /// clear can be retried and still find every chunk.
    fn clear(&self) -> Result<(), StoreError> {
        for (slot, recorded) in self.recorded_counts()? {
            let end = self.sweep_end(slot, recorded)?;
            self.delete_range(slot, 0..end)?;
        }
        self.backend.delete(LEGACY_COUNT_ENTRY)?;
        self.backend.delete(CURRENT_ENTRY)
    }
}

/// `keyring` entries under one service name.
pub struct KeyringBackend {
    service: &'static str,
}

impl KeyringBackend {
    pub fn new(service: &'static str) -> Self {
        Self { service }
    }

    fn entry(&self, name: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(self.service, name)
            .map_err(|error| StoreError::Keyring(describe(&error)))
    }
}

/// A fixed description of a keyring error. Never formats the error itself:
/// `BadEncoding` and `BadDataFormat` carry the stored bytes.
fn describe(error: &keyring::Error) -> &'static str {
    match error {
        keyring::Error::PlatformFailure(_) => "platform failure",
        keyring::Error::NoStorageAccess(_) => "no storage access",
        keyring::Error::NoEntry => "no entry",
        keyring::Error::BadEncoding(_) => "bad encoding",
        keyring::Error::BadDataFormat(..) => "bad data format",
        keyring::Error::BadStoreFormat(_) => "bad store format",
        keyring::Error::TooLong(..) => "value too long",
        keyring::Error::Invalid(..) => "invalid attribute",
        keyring::Error::Ambiguous(_) => "ambiguous entry",
        keyring::Error::NoDefaultStore => "no default store",
        keyring::Error::NotSupportedByStore(_) => "not supported by store",
        _ => "unknown keyring error",
    }
}

impl EntryBackend for KeyringBackend {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.entry(name)?.get_secret() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(StoreError::Keyring(describe(&error))),
        }
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), StoreError> {
        self.entry(name)?
            .set_secret(value)
            .map_err(|error| StoreError::Keyring(describe(&error)))
    }

    fn delete(&self, name: &str) -> Result<(), StoreError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(StoreError::Keyring(describe(&error))),
        }
    }
}

/// Debug-only plain-text store. Never selected in release builds.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SessionStore for FileStore {
    fn load(&self) -> Result<Option<Secret>, StoreError> {
        match fs::read_to_string(&self.path) {
            Ok(text) if text.trim().is_empty() => Ok(None),
            Ok(text) => Ok(Some(Secret::new(text.trim()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, header: &Secret) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_private(&self.path, header.expose())?;
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.set_len(0)?;
    file.write_all(contents.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    fs::write(path, contents)
}

/// Which store `store_for` builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    Keyring,
    File,
}

/// `KASET_SESSION_STORE=file` selects the file store in debug builds only.
pub fn store_kind(env_value: Option<&str>, debug_build: bool) -> StoreKind {
    match env_value {
        Some("file") if debug_build => StoreKind::File,
        _ => StoreKind::Keyring,
    }
}

pub fn store_for(kind: StoreKind, app_data_dir: &Path) -> Box<dyn SessionStore> {
    match kind {
        StoreKind::Keyring => Box::new(ChunkedStore::new(KeyringBackend::new(KEYRING_SERVICE))),
        StoreKind::File => Box::new(FileStore::new(app_data_dir.join(DEV_FILE_NAME))),
    }
}

/// In-memory store for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    value: std::sync::Mutex<Option<Secret>>,
    fail_saves: bool,
}

#[cfg(test)]
impl MemoryStore {
    /// A store whose `save` always fails, like a locked Keychain.
    pub fn failing() -> Self {
        Self {
            fail_saves: true,
            ..Self::default()
        }
    }
}

#[cfg(test)]
impl SessionStore for MemoryStore {
    fn load(&self) -> Result<Option<Secret>, StoreError> {
        Ok(self.value.lock().unwrap().clone())
    }

    fn save(&self, header: &Secret) -> Result<(), StoreError> {
        if self.fail_saves {
            return Err(StoreError::Keyring("platform failure"));
        }
        *self.value.lock().unwrap() = Some(header.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        *self.value.lock().unwrap() = None;
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests and checks**

Run: `cargo test -p kaset session::store`
Expected: `test result: ok. 19 passed`.

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all pass (the `kaset` lib reports 50 tests).

- [ ] **Step 5: Commit and push**

```bash
git add app/src-tauri/src/session/store.rs
git commit -m "fix(app): make keyring session saves atomic and cap stored chunk counts"
git push origin main
```

---

### Task 2: Session epochs, expired-jar rejection, launch gate, blocking cookie work

Closes three races (a login completing after sign-out, a stale 401 expiring a newer session, a 401 before the launch refresh), the re-sign-in loop with the jar that just expired, and moves `Webview::cookies()` and keyring I/O off async workers so the player (Task 4) can refresh cookies repeatedly.

**Files:**
- Modify: `app/src-tauri/Cargo.toml`, `app/src-tauri/src/session/mod.rs`, `app/src-tauri/src/state.rs`, `app/src-tauri/src/login.rs`, `app/src-tauri/src/commands.rs`, `app/src-tauri/src/lib.rs`
- Create: `app/src-tauri/src/webview_session.rs`
- Regenerated: `app/src/bindings.ts` (doc comment of `getHome`)
- Test: `app/src-tauri/src/session/mod.rs`, `app/src-tauri/src/state.rs`

**Interfaces:**
- Consumes: Task 1 store (unchanged API); existing `CookieJar`, `SharedCookies`, `youtube_cookie_jar`, `session_cookies`, `emit_auth_changed`, `close_login_window`, `LOGIN_LABEL`.
- Produces:
  - `SessionError::{NoSapisid, Stale, SameAsExpired, Store(StoreError)}`.
  - `Session::epoch(&self) -> u64` (never blocks on store I/O), `Session::sign_in_if(&self, epoch: u64, jar: CookieJar) -> Result<Option<AuthStatus>, SessionError>`, `Session::sign_in(&self, jar) -> Result<Option<AuthStatus>, SessionError>` (= `sign_in_if(self.epoch(), jar)`), `Session::mark_expired_if(&self, epoch: u64) -> Option<AuthStatus>`. `Session::mark_expired()` is removed. `restore`, `refresh`, `sign_out`, `status`, `cookies` keep their signatures.
  - `kaset_lib::state::{LaunchGate, LAUNCH_REFRESH_TIMEOUT}`: `LaunchGate::new()`, `open(&self)`, `async fn wait(&self, timeout: Duration) -> bool`; `AppState { pub session: Session, pub launch: LaunchGate, .. }`.
  - `kaset_lib::webview_session::{blocking, refresh_session_from_webview}`: `async fn blocking<T, F: FnOnce() -> T + Send + 'static>(work: F) -> Result<T, AppError>`; `async fn refresh_session_from_webview(app: &AppHandle, label: &str, reason: &'static str) -> bool`.
  - `commands::sign_out(app)` and `commands::get_home(app, state)` keep their TS signatures (`signOut()`, `getHome()`).

- [ ] **Step 1: Write the failing session tests**

In `app/src-tauri/src/session/mod.rs`, replace everything from `#[cfg(test)]` / `mod tests {` to the end of the file with:

```rust
#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::thread;
    use std::time::Duration;

    use super::*;
    use store::{MemoryStore, SessionStore};
    use ytm_core::auth::CookieJar;
    use ytm_core::secret::Secret;

    fn jar(sapisid: &str) -> CookieJar {
        CookieJar::from_pairs([("SAPISID", sapisid), ("HSID", "test-cookie")])
    }

    fn session_with(store: MemoryStore) -> Session {
        Session::new(Box::new(store))
    }

    struct BlockingSaveStore {
        inner: MemoryStore,
        save_started: Sender<()>,
        release_save: Mutex<Receiver<()>>,
    }

    impl SessionStore for BlockingSaveStore {
        fn load(&self) -> Result<Option<Secret>, StoreError> {
            self.inner.load()
        }

        fn save(&self, header: &Secret) -> Result<(), StoreError> {
            self.save_started.send(()).unwrap();
            self.release_save
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            self.inner.save(header)
        }

        fn clear(&self) -> Result<(), StoreError> {
            self.inner.clear()
        }
    }

    struct NoSaveStore(MemoryStore);

    impl SessionStore for NoSaveStore {
        fn load(&self) -> Result<Option<Secret>, StoreError> {
            self.0.load()
        }

        fn save(&self, _: &Secret) -> Result<(), StoreError> {
            panic!("an empty WebView cookie jar must never be saved");
        }

        fn clear(&self) -> Result<(), StoreError> {
            self.0.clear()
        }
    }

    #[derive(Default)]
    struct CountingStore {
        inner: MemoryStore,
        saves: Arc<AtomicUsize>,
    }

    impl SessionStore for CountingStore {
        fn load(&self) -> Result<Option<Secret>, StoreError> {
            self.inner.load()
        }

        fn save(&self, header: &Secret) -> Result<(), StoreError> {
            self.saves.fetch_add(1, Ordering::Relaxed);
            self.inner.save(header)
        }

        fn clear(&self) -> Result<(), StoreError> {
            self.inner.clear()
        }
    }

    fn restored_counting_session(original: &CookieJar) -> (Session, Arc<AtomicUsize>) {
        let store = CountingStore::default();
        store.inner.save(original.header()).unwrap();
        let saves = Arc::clone(&store.saves);
        let session = Session::new(Box::new(store));
        assert_eq!(session.restore(), AuthStatus::SignedIn);
        (session, saves)
    }

    #[test]
    fn starts_signed_out() {
        let session = session_with(MemoryStore::default());
        assert_eq!(session.status(), AuthStatus::SignedOut);
        assert!(session.cookies().get().is_none());
    }

    #[test]
    fn restore_without_stored_session_is_signed_out() {
        let session = session_with(MemoryStore::default());
        assert_eq!(session.restore(), AuthStatus::SignedOut);
    }

    #[test]
    fn restore_with_stored_session_signs_in() {
        let store = MemoryStore::default();
        store
            .save(&Secret::new("SAPISID=test-sapisid; HSID=test-cookie"))
            .unwrap();
        let session = session_with(store);
        assert_eq!(session.restore(), AuthStatus::SignedIn);
        assert_eq!(
            session.cookies().get().and_then(|jar| jar.sapisid()),
            Some(Secret::new("test-sapisid"))
        );
    }

    #[test]
    fn restore_ignores_stored_header_without_sapisid() {
        let store = MemoryStore::default();
        store.save(&Secret::new("HSID=test-cookie")).unwrap();
        let session = session_with(store);
        assert_eq!(session.restore(), AuthStatus::SignedOut);
        assert!(session.cookies().get().is_none());
    }

    #[test]
    fn sign_in_sets_cookies_persists_and_reports_change() {
        let session = session_with(MemoryStore::default());
        assert_eq!(
            session.sign_in(jar("test-sapisid")).unwrap(),
            Some(AuthStatus::SignedIn)
        );
        assert_eq!(session.cookies().get(), Some(jar("test-sapisid")));
        assert_eq!(
            session.store.load().unwrap(),
            Some(Secret::new("SAPISID=test-sapisid; HSID=test-cookie"))
        );
        // A second completion (the page can finish loading twice) is not a change.
        assert_eq!(session.sign_in(jar("test-sapisid")).unwrap(), None);
    }

    #[test]
    fn sign_in_without_sapisid_is_rejected() {
        let session = session_with(MemoryStore::default());
        let result = session.sign_in(CookieJar::from_pairs([("HSID", "test-cookie")]));
        assert!(matches!(result, Err(SessionError::NoSapisid)));
        assert_eq!(session.status(), AuthStatus::SignedOut);
    }

    #[test]
    fn sign_in_survives_a_failing_store() {
        let session = session_with(MemoryStore::failing());
        assert_eq!(
            session.sign_in(jar("test-sapisid")).unwrap(),
            Some(AuthStatus::SignedIn)
        );
        assert!(session.cookies().get().is_some());
    }

    #[test]
    fn expiry_only_applies_when_signed_in() {
        let session = session_with(MemoryStore::default());
        assert_eq!(session.mark_expired_if(session.epoch()), None);
        session.sign_in(jar("test-sapisid")).unwrap();
        assert_eq!(
            session.mark_expired_if(session.epoch()),
            Some(AuthStatus::Expired)
        );
        assert_eq!(session.mark_expired_if(session.epoch()), None);
        // Signing in again recovers from expiry.
        assert_eq!(
            session.sign_in(jar("test-sapisid-2")).unwrap(),
            Some(AuthStatus::SignedIn)
        );
    }

    #[test]
    fn sign_out_clears_memory_and_store() {
        let session = session_with(MemoryStore::default());
        session.sign_in(jar("test-sapisid")).unwrap();
        assert_eq!(session.sign_out().unwrap(), Some(AuthStatus::SignedOut));
        assert!(session.cookies().get().is_none());
        assert_eq!(session.store.load().unwrap(), None);
        assert_eq!(session.sign_out().unwrap(), None);
    }

    #[test]
    fn refresh_updates_rotated_cookies_only_when_signed_in() {
        let session = session_with(MemoryStore::default());
        assert!(!session.refresh(jar("test-sapisid")));
        assert!(session.cookies().get().is_none());

        session.sign_in(jar("test-sapisid")).unwrap();
        assert!(!session.refresh(jar("test-sapisid")));
        assert!(session.refresh(jar("test-sapisid-rotated")));
        assert_eq!(session.cookies().get(), Some(jar("test-sapisid-rotated")));
        assert_eq!(
            session.store.load().unwrap(),
            Some(Secret::new(
                "SAPISID=test-sapisid-rotated; HSID=test-cookie"
            ))
        );
        assert!(!session.refresh(CookieJar::from_pairs([("HSID", "test-cookie")])));
    }

    #[test]
    fn refresh_updates_rotation_only_cookies_without_saving() {
        let original = CookieJar::from_header("SAPISID=test-sapisid; SIDCC=test-old");
        let updated = CookieJar::from_header("SAPISID=test-sapisid; SIDCC=test-new");
        let (session, saves) = restored_counting_session(&original);

        assert!(session.refresh(updated.clone()));
        assert_eq!(session.cookies().get(), Some(updated));
        assert_eq!(saves.load(Ordering::Relaxed), 0);
        assert_eq!(
            session.store.load().unwrap().as_ref(),
            Some(original.header())
        );
    }

    #[test]
    fn refresh_reorders_legacy_auth_cookies_without_saving() {
        let original = CookieJar::from_header("SAPISID=test-sapisid; HSID=test-cookie");
        let updated = CookieJar::from_header("HSID=test-cookie; SAPISID=test-sapisid");
        let (session, saves) = restored_counting_session(&original);

        assert!(session.refresh(updated.clone()));
        assert_eq!(session.cookies().get(), Some(updated));
        assert_eq!(saves.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn refresh_saves_a_changed_sapisid_once() {
        let (session, saves) = restored_counting_session(&jar("test-old"));
        let updated = jar("test-new");

        assert!(session.refresh(updated.clone()));
        assert!(!session.refresh(updated.clone()));
        assert_eq!(saves.load(Ordering::Relaxed), 1);
        assert_eq!(
            session.store.load().unwrap().as_ref(),
            Some(updated.header())
        );
    }

    #[test]
    fn sign_out_clears_a_concurrent_refresh() {
        let (save_started, wait_for_save) = mpsc::channel();
        let (release_save, wait_for_release) = mpsc::channel();
        let inner = MemoryStore::default();
        inner.save(jar("test-sapisid").header()).unwrap();
        let session = Session::new(Box::new(BlockingSaveStore {
            inner,
            save_started,
            release_save: Mutex::new(wait_for_release),
        }));
        assert_eq!(session.restore(), AuthStatus::SignedIn);

        thread::scope(|scope| {
            let refresh = scope.spawn(|| session.refresh(jar("test-sapisid-rotated")));
            wait_for_save.recv_timeout(Duration::from_secs(5)).unwrap();

            let (sign_out_started, wait_for_sign_out) = mpsc::channel();
            let (sign_out_finished, wait_for_finish) = mpsc::channel();
            let session = &session;
            let sign_out = scope.spawn(move || {
                sign_out_started.send(()).unwrap();
                let result = session.sign_out();
                sign_out_finished.send(()).unwrap();
                result
            });
            wait_for_sign_out
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            // The old code can complete sign-out while save is parked. The
            // fixed code waits, so release save after a bounded wait.
            let _ = wait_for_finish.recv_timeout(Duration::from_secs(1));
            release_save.send(()).unwrap();

            assert!(refresh.join().unwrap());
            assert_eq!(
                sign_out.join().unwrap().unwrap(),
                Some(AuthStatus::SignedOut)
            );
        });

        assert_eq!(session.status(), AuthStatus::SignedOut);
        assert_eq!(
            (
                session.cookies().get().is_none(),
                session.store.load().unwrap().is_none()
            ),
            (true, true),
            "sign-out must clear both shared cookies and the persisted session"
        );
    }

    #[test]
    fn empty_webview_jar_leaves_the_session_unchanged_without_saving() {
        let original = jar("test-sapisid");
        let store = MemoryStore::default();
        store.save(original.header()).unwrap();
        let session = Session::new(Box::new(NoSaveStore(store)));
        assert_eq!(session.restore(), AuthStatus::SignedIn);

        let empty = cookies::youtube_cookie_jar(&[]);
        assert!(matches!(
            session.sign_in(empty.clone()),
            Err(SessionError::NoSapisid)
        ));
        assert!(!session.refresh(empty));
        assert_eq!(session.status(), AuthStatus::SignedIn);
        assert_eq!(session.cookies().get().as_ref(), Some(&original));
        assert_eq!(
            session.store.load().unwrap().as_ref(),
            Some(original.header())
        );
    }

    #[test]
    fn shared_cookie_handle_follows_session_changes() {
        let session = session_with(MemoryStore::default());
        let handle = session.cookies().clone();
        session.sign_in(jar("test-sapisid")).unwrap();
        assert!(handle.get().is_some());
        session.sign_out().unwrap();
        assert!(handle.get().is_none());
    }

    #[test]
    fn epoch_changes_on_identity_changes_only() {
        let session = session_with(MemoryStore::default());
        let start = session.epoch();
        session.restore();
        let restored = session.epoch();
        assert!(restored > start);
        session.sign_in(jar("test-sapisid")).unwrap();
        let signed_in = session.epoch();
        assert!(signed_in > restored);
        assert!(session.refresh(jar("test-sapisid-rotated")));
        assert_eq!(session.epoch(), signed_in, "rotation keeps the epoch");
        session.mark_expired_if(signed_in);
        assert_eq!(session.epoch(), signed_in, "expiry keeps the epoch");
        session.sign_out().unwrap();
        assert!(session.epoch() > signed_in);
    }

    #[test]
    fn sign_in_that_started_before_sign_out_is_discarded() {
        let session = session_with(MemoryStore::default());
        let barrier = Barrier::new(2);
        thread::scope(|scope| {
            let login = scope.spawn(|| {
                // The login page finished loading: capture the epoch, then
                // read cookies while the user signs out.
                let epoch = session.epoch();
                barrier.wait();
                barrier.wait();
                session.sign_in_if(epoch, jar("test-sapisid"))
            });
            barrier.wait();
            session.sign_out().unwrap();
            barrier.wait();
            assert!(matches!(login.join().unwrap(), Err(SessionError::Stale)));
        });
        assert_eq!(session.status(), AuthStatus::SignedOut);
        assert!(session.cookies().get().is_none());
        assert_eq!(session.store.load().unwrap(), None);
    }

    #[test]
    fn stale_auth_error_does_not_expire_a_newer_session() {
        let session = session_with(MemoryStore::default());
        session.sign_in(jar("test-sapisid")).unwrap();
        let barrier = Barrier::new(2);
        thread::scope(|scope| {
            let request = scope.spawn(|| {
                // get_home captured the epoch, then its 401 arrives after the
                // user signed out and in again.
                let epoch = session.epoch();
                barrier.wait();
                barrier.wait();
                session.mark_expired_if(epoch)
            });
            barrier.wait();
            session.sign_out().unwrap();
            session.sign_in(jar("test-sapisid-2")).unwrap();
            barrier.wait();
            assert_eq!(request.join().unwrap(), None);
        });
        assert_eq!(session.status(), AuthStatus::SignedIn);
        assert_eq!(session.cookies().get(), Some(jar("test-sapisid-2")));
    }

    #[test]
    fn current_auth_error_expires_the_session() {
        let session = session_with(MemoryStore::default());
        session.sign_in(jar("test-sapisid")).unwrap();
        let epoch = session.epoch();
        assert_eq!(session.mark_expired_if(epoch), Some(AuthStatus::Expired));
        assert_eq!(session.status(), AuthStatus::Expired);
    }

    #[test]
    fn the_jar_that_expired_cannot_sign_in_again() {
        let session = session_with(MemoryStore::default());
        session.sign_in(jar("test-sapisid")).unwrap();
        session.mark_expired_if(session.epoch());

        // The WebView still holds the expired auth cookies; only a non-auth
        // cookie rotated.
        let same = CookieJar::from_pairs([
            ("HSID", "test-cookie"),
            ("SAPISID", "test-sapisid"),
            ("SIDCC", "test-rotated"),
        ]);
        assert!(matches!(
            session.sign_in_if(session.epoch(), same),
            Err(SessionError::SameAsExpired)
        ));
        assert_eq!(session.status(), AuthStatus::Expired);

        assert_eq!(
            session
                .sign_in_if(session.epoch(), jar("test-sapisid-2"))
                .unwrap(),
            Some(AuthStatus::SignedIn)
        );
        // After a successful sign-in the old jar is forgotten.
        session.sign_out().unwrap();
        assert_eq!(
            session.sign_in(jar("test-sapisid")).unwrap(),
            Some(AuthStatus::SignedIn)
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset session::tests`
Expected: compile errors such as `no method named 'mark_expired_if' found for struct 'Session'`, `no method named 'epoch'`, `no variant named 'Stale'`.

- [ ] **Step 3: Implement epochs and the expired jar**

Replace everything above `#[cfg(test)]` / `mod tests {` in `app/src-tauri/src/session/mod.rs` with:

```rust
//! Sign-in state and cookie ownership. No Tauri types: the window and event
//! glue lives in `login.rs` and `commands.rs`, so this is testable alone.

pub mod cookies;
mod status;
pub mod store;

pub use status::AuthStatus;

use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard, PoisonError};

use store::{SessionStore, StoreError};
use ytm_core::auth::{CookieJar, SharedCookies};

const AUTH_COOKIE_NAMES: &[&str] = &[
    "SID",
    "HSID",
    "SSID",
    "APISID",
    "SAPISID",
    "__Secure-1PAPISID",
    "__Secure-3PAPISID",
    "__Secure-1PSID",
    "__Secure-3PSID",
];

fn auth_cookie_pairs(jar: &CookieJar) -> BTreeSet<(&str, &str)> {
    jar.pairs()
        .filter(|(name, _)| AUTH_COOKIE_NAMES.contains(name))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("the sign-in cookies have no SAPISID")]
    NoSapisid,
    #[error("the session changed after this operation started")]
    Stale,
    #[error("the sign-in cookies are the ones that expired")]
    SameAsExpired,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Sign-in state guarded by one mutex. `epoch` changes whenever the session's
/// identity changes (restore, sign-in, sign-out), so work that started under
/// an older epoch can be discarded.
struct State {
    status: AuthStatus,
    epoch: u64,
    /// The cookies in use when the API last answered 401/403.
    expired_jar: Option<CookieJar>,
}

/// Owns the cookies the API client sends and the persisted copy.
///
/// A return value of `Some(new_status)` signals that the caller should emit
/// `auth://changed`.
pub struct Session {
    cookies: SharedCookies,
    state: Mutex<State>,
    store: Box<dyn SessionStore>,
    // Serializes status decisions, store I/O, and shared cookie updates.
    transition: Mutex<()>,
}

impl Session {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            cookies: SharedCookies::default(),
            state: Mutex::new(State {
                status: AuthStatus::SignedOut,
                epoch: 0,
                expired_jar: None,
            }),
            store,
            transition: Mutex::new(()),
        }
    }

    /// The handle to give `InnerTubeClient::with_shared_cookies`.
    pub fn cookies(&self) -> &SharedCookies {
        &self.cookies
    }

    pub fn status(&self) -> AuthStatus {
        self.lock_state().status
    }

    /// The current session identity. Capture it before slow work (reading
    /// login cookies, an API request) and pass it to `sign_in_if` or
    /// `mark_expired_if`. Never blocks on store I/O.
    pub fn epoch(&self) -> u64 {
        self.lock_state().epoch
    }

    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_transition(&self) -> MutexGuard<'_, ()> {
        self.transition
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Sets the status, starting a new epoch when `new_epoch` is true.
    fn set_status(&self, next: AuthStatus, new_epoch: bool) -> Option<AuthStatus> {
        let mut state = self.lock_state();
        if new_epoch {
            state.epoch += 1;
            state.expired_jar = None;
        }
        (state.status != next).then(|| {
            state.status = next;
            next
        })
    }

    /// Loads the stored session at launch. A store error or a stored header
    /// without SAPISID leaves the user signed out.
    pub fn restore(&self) -> AuthStatus {
        let _transition = self.lock_transition();
        let jar = match self.store.load() {
            Ok(Some(header)) => Some(CookieJar::from_header(header.expose())),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(%error, "could not load the stored session");
                None
            }
        };
        match jar.filter(|jar| jar.sapisid().is_some()) {
            Some(jar) => {
                self.cookies.set(Some(jar));
                self.set_status(AuthStatus::SignedIn, true);
            }
            None => {
                self.cookies.set(None);
                self.set_status(AuthStatus::SignedOut, true);
            }
        }
        self.status()
    }

    /// Completes a sign-in with cookies read from the login WebView, unless the
    /// session changed since `epoch` was captured (`Stale`) or the cookies are
    /// the ones that just expired (`SameAsExpired`). A failed save keeps the
    /// in-memory session and is logged; the user stays signed in until the app
    /// quits.
    pub fn sign_in_if(
        &self,
        epoch: u64,
        jar: CookieJar,
    ) -> Result<Option<AuthStatus>, SessionError> {
        let _transition = self.lock_transition();
        if jar.sapisid().is_none() {
            return Err(SessionError::NoSapisid);
        }
        {
            let state = self.lock_state();
            if state.epoch != epoch {
                return Err(SessionError::Stale);
            }
            if state
                .expired_jar
                .as_ref()
                .is_some_and(|expired| auth_cookie_pairs(expired) == auth_cookie_pairs(&jar))
            {
                return Err(SessionError::SameAsExpired);
            }
        }
        if let Err(error) = self.store.save(jar.header()) {
            tracing::warn!(%error, "signed in, but the session could not be saved");
        }
        self.cookies.set(Some(jar));
        Ok(self.set_status(AuthStatus::SignedIn, true))
    }

    /// `sign_in_if` with the current epoch.
    pub fn sign_in(&self, jar: CookieJar) -> Result<Option<AuthStatus>, SessionError> {
        self.sign_in_if(self.epoch(), jar)
    }

    /// Picks up rotated cookies from the WebView. Ignored unless signed in,
    /// so it can never sign a user back in after sign-out. Only changes to the
    /// auth cookie set trigger a save. The epoch is unchanged: rotation keeps
    /// the same session.
    pub fn refresh(&self, jar: CookieJar) -> bool {
        let _transition = self.lock_transition();
        if self.status() != AuthStatus::SignedIn || jar.sapisid().is_none() {
            return false;
        }
        let previous = self.cookies.get();
        if previous.as_ref() == Some(&jar) {
            return false;
        }
        if previous
            .as_ref()
            .is_none_or(|old| auth_cookie_pairs(old) != auth_cookie_pairs(&jar))
            && let Err(error) = self.store.save(jar.header())
        {
            tracing::warn!(%error, "could not save refreshed cookies");
        }
        self.cookies.set(Some(jar));
        true
    }

    /// The API answered 401/403 for a request that started at `epoch`. Expires
    /// the session only if it is still that signed-in session. Keeps the
    /// cookies (a new sign-in replaces them) and remembers them so the same
    /// stale jar cannot sign in again.
    pub fn mark_expired_if(&self, epoch: u64) -> Option<AuthStatus> {
        let _transition = self.lock_transition();
        let mut state = self.lock_state();
        if state.epoch != epoch || state.status != AuthStatus::SignedIn {
            return None;
        }
        state.expired_jar = self.cookies.get();
        state.status = AuthStatus::Expired;
        Some(AuthStatus::Expired)
    }

    /// Forgets the session in memory and in the store, and starts a new epoch.
    /// The in-memory state is cleared even when the store fails, and the error
    /// is returned.
    pub fn sign_out(&self) -> Result<Option<AuthStatus>, SessionError> {
        let _transition = self.lock_transition();
        self.cookies.set(None);
        let changed = self.set_status(AuthStatus::SignedOut, true);
        self.store.clear()?;
        Ok(changed)
    }
}
```

- [ ] **Step 4: Add the launch gate**

Replace `app/src-tauri/Cargo.toml` with (the `tokio` dependency gains `time`; `[dev-dependencies]` gains `tokio` with `macros` and `rt`):

```toml
[package]
name = "kaset"
version = "0.1.0"
description = "Kaset, a YouTube Music desktop client"
edition.workspace = true
license.workspace = true

[lib]
name = "kaset_lib"
crate-type = ["rlib"]

[build-dependencies]
tauri-build = { version = "2.7.6", features = [] }

[dependencies]
keyring = "4.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
specta = { version = "=2.0.0-rc.28", features = ["derive"] }
specta-typescript = "=0.0.15"
tauri = { version = "2.12.7", features = [] }
tauri-specta = { version = "=2.0.0-rc.29", features = ["derive", "typescript"] }
thiserror = "2"
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
ytm-core = { version = "0.1.0", path = "../../crates/ytm-core", features = ["specta"] }

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["macros", "rt"] }
```

Replace `app/src-tauri/src/state.rs` with:

```rust
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OnceCell, watch};
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};

use crate::error::AppError;
use crate::session::Session;

/// How long API commands wait for the launch cookie refresh.
pub const LAUNCH_REFRESH_TIMEOUT: Duration = Duration::from_secs(5);

/// Opens once the launch cookie refresh has finished, so the first API request
/// uses rotated cookies instead of expiring the session with stale stored ones.
pub struct LaunchGate {
    open: watch::Sender<bool>,
}

impl LaunchGate {
    pub fn new() -> Self {
        Self {
            open: watch::Sender::new(false),
        }
    }

    pub fn open(&self) {
        self.open.send_replace(true);
    }

    /// True once the gate is open; false if `timeout` passes first.
    pub async fn wait(&self, timeout: Duration) -> bool {
        let mut open = self.open.subscribe();
        tokio::time::timeout(timeout, open.wait_for(|open| *open))
            .await
            .is_ok_and(|result| result.is_ok())
    }
}

impl Default for LaunchGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Managed Tauri state.
pub struct AppState {
    pub session: Session,
    pub launch: LaunchGate,
    client: OnceCell<Arc<InnerTubeClient>>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            launch: LaunchGate::new(),
            client: OnceCell::new(),
        }
    }

    /// The API client, built on first use. It shares the session's cookie
    /// handle, so sign-in and sign-out take effect without rebuilding it.
    /// `KASET_YTMUSIC_API_KEY` overrides the key fetched from the web client.
    pub async fn client(&self) -> Result<Arc<InnerTubeClient>, AppError> {
        self.client
            .get_or_try_init(|| async {
                let api_key = match std::env::var(API_KEY_ENV) {
                    Ok(key) if !key.trim().is_empty() => key,
                    _ => fetch_api_key(MUSIC_ORIGIN).await?,
                };
                let client = InnerTubeClient::with_shared_cookies(
                    ClientConfig::music(api_key),
                    self.session.cookies().clone(),
                )?;
                Ok::<_, AppError>(Arc::new(client))
            })
            .await
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn launch_gate_waits_until_opened() {
        let gate = Arc::new(LaunchGate::new());
        let waiter = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.wait(Duration::from_secs(5)).await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        gate.open();
        assert!(waiter.await.unwrap());
    }

    #[tokio::test]
    async fn launch_gate_returns_at_once_when_open() {
        let gate = LaunchGate::new();
        gate.open();
        assert!(gate.wait(Duration::from_millis(1)).await);
    }

    #[tokio::test]
    async fn launch_gate_times_out() {
        assert!(!LaunchGate::new().wait(Duration::from_millis(10)).await);
    }
}
```

- [ ] **Step 5: Add the blocking helpers**

Create `app/src-tauri/src/webview_session.rs`:

```rust
//! Moves WebView cookie reads and session store I/O off the async workers.
//!
//! `Webview::cookies()` blocks until the main thread answers (and deadlocks on
//! Windows when called from the main thread), and keyring calls block on the
//! OS store, so both run on Tauri's blocking pool.

use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::session::cookies::youtube_cookie_jar;
use crate::state::AppState;

/// Runs `work` on the blocking pool.
pub async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(AppError::from)
}

/// Reads the shared cookie store through the `label` window and passes the
/// YouTube cookies to `Session::refresh`. Runs at launch and after every
/// player page load. Returns true when the in-memory cookies changed.
pub async fn refresh_session_from_webview(
    app: &AppHandle,
    label: &str,
    reason: &'static str,
) -> bool {
    let Some(window) = app.get_webview_window(label) else {
        return false;
    };
    let task_app = app.clone();
    let result = blocking(move || {
        let cookies = window.cookies()?;
        let state = task_app.state::<AppState>();
        Ok::<_, tauri::Error>(state.session.refresh(youtube_cookie_jar(&cookies)))
    })
    .await;
    match result {
        Ok(Ok(changed)) => {
            if changed {
                tracing::info!(reason, "refreshed session cookies from the WebView");
            }
            changed
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, reason, "could not read WebView cookies");
            false
        }
        Err(error) => {
            tracing::warn!(%error, reason, "cookie refresh task failed");
            false
        }
    }
}
```

- [ ] **Step 6: Use epochs and the blocking pool in login, commands, and launch**

`commands.rs` still calls the removed `mark_expired()`, so the crate does not build until this step is done.

In `app/src-tauri/src/login.rs`, replace everything above `#[cfg(test)]` / `mod tests {` with (the tests module is unchanged):

```rust
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
use crate::session::cookies::{is_country_google, youtube_cookie_jar};
use crate::session::{AuthStatus, SessionError};
use crate::state::AppState;
use crate::webview_session::blocking;

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
                // Captured before the cookie read: a sign-out in between makes
                // this completion stale.
                let epoch = app.state::<AppState>().session.epoch();
                tauri::async_runtime::spawn(async move { complete_sign_in(app, epoch).await });
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_USER_AGENT);
    builder.build()?;
    Ok(())
}

/// Reads the login window's cookies and signs in when SAPISID is present and
/// the session has not changed since `epoch`. The cookie read and the store
/// save run on the blocking pool.
async fn complete_sign_in(app: AppHandle, epoch: u64) {
    let Some(window) = app.get_webview_window(LOGIN_LABEL) else {
        return;
    };
    let reader = window.clone();
    let task_app = app.clone();
    let result = blocking(move || {
        let cookies = reader.cookies()?;
        let state = task_app.state::<AppState>();
        Ok::<_, tauri::Error>(
            state
                .session
                .sign_in_if(epoch, youtube_cookie_jar(&cookies)),
        )
    })
    .await;
    match result {
        Ok(Ok(Ok(changed))) => {
            if let Some(status) = changed {
                emit_auth_changed(&app, status);
            }
            close_login_window(&window);
        }
        Ok(Ok(Err(SessionError::NoSapisid))) => {
            tracing::debug!("music.youtube.com loaded without SAPISID; waiting for sign-in");
        }
        Ok(Ok(Err(SessionError::SameAsExpired))) => {
            tracing::info!(
                "the WebView still holds the expired session; waiting for a new sign-in"
            );
        }
        Ok(Ok(Err(SessionError::Stale))) => {
            tracing::debug!("discarded a sign-in that started before a session change");
        }
        Ok(Ok(Err(error))) => tracing::warn!(%error, "sign-in failed"),
        Ok(Err(error)) => tracing::warn!(%error, "could not read login cookies"),
        Err(error) => tracing::warn!(%error, "sign-in task failed"),
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
```

Replace `app/src-tauri/src/commands.rs` with:

```rust
use tauri::{AppHandle, Manager, State};
use ytm_core::models::HomePage;
use ytm_core::parsers::home::parse_home;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, close_login_window, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::session::cookies::session_cookies;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::blocking;

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

/// Forgets the stored session and deletes the WebView's Google cookies.
#[tauri::command]
#[specta::specta]
pub async fn sign_out(app: AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        close_login_window(&window);
    }
    let task_app = app.clone();
    let result = blocking(move || {
        let result = task_app.state::<AppState>().session.sign_out();
        clear_webview_cookies(&task_app);
        result
    })
    .await?;
    emit_auth_changed(&app, AuthStatus::SignedOut);
    result
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
}

/// Deletes YouTube and Google session cookies from the shared WebView store,
/// re-reading up to three times like the Swift app's cookie clearer. Blocking:
/// call it on the blocking pool.
fn clear_webview_cookies(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    for _ in 0..3 {
        let remaining = match window.cookies() {
            Ok(cookies) => session_cookies(cookies),
            Err(error) => {
                tracing::warn!(%error, "could not read cookies during sign-out");
                return;
            }
        };
        if remaining.is_empty() {
            return;
        }
        for cookie in remaining {
            if let Err(error) = window.delete_cookie(cookie) {
                tracing::warn!(%error, "could not delete a cookie during sign-out");
            }
        }
    }
    tracing::warn!("Google cookies remained in the WebView after sign-out");
}

/// Fetches and parses the signed-in home feed. Waits for the launch cookie
/// refresh first, and expires the session on 401/403 only if it is still the
/// session the request started with.
#[tauri::command]
#[specta::specta]
pub async fn get_home(app: AppHandle, state: State<'_, AppState>) -> Result<HomePage, AppError> {
    if !state.launch.wait(LAUNCH_REFRESH_TIMEOUT).await {
        tracing::warn!("launch cookie refresh did not finish in time");
    }
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    let epoch = state.session.epoch();
    let result = async {
        let client = state.client().await?;
        let response = client.browse("FEmusic_home").await?;
        Ok::<_, AppError>(parse_home(&response)?)
    }
    .await;
    result.map_err(|error| on_error(&app, &state, epoch, error))
}

/// Moves the session to `Expired` on an auth error for a request that started
/// at `epoch`, and passes the error on.
fn on_error(app: &AppHandle, state: &AppState, epoch: u64, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired
        && let Some(status) = state.session.mark_expired_if(epoch)
    {
        emit_auth_changed(app, status);
    }
    error
}
```

Replace `app/src-tauri/src/lib.rs` with:

```rust
pub mod commands;
pub mod error;
pub mod events;
pub mod login;
pub mod session;
pub mod state;
pub mod webview_session;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::session::Session;
use crate::session::store::{STORE_ENV, store_for, store_kind};
use crate::state::AppState;
use crate::webview_session::refresh_session_from_webview;

/// Commands and events exposed to the UI. `bindings_are_current` exports
/// them to `app/src/bindings.ts`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::auth_status,
            commands::sign_in,
            commands::sign_out,
            commands::get_home,
        ])
        .events(collect_events![events::AuthChanged])
        .disable_serde_phases()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kaset_lib=info,ytm_core=info".into()),
        )
        .init();

    let builder = specta_builder();
    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let data_dir = app.path().app_data_dir()?;
            let kind = store_kind(
                std::env::var(STORE_ENV).ok().as_deref(),
                cfg!(debug_assertions),
            );
            tracing::info!(store = ?kind, "session store selected");
            let session = Session::new(store_for(kind, &data_dir));
            let status = session.restore();
            tracing::info!(?status, "session restored");
            app.manage(AppState::new(session));
            refresh_cookies_from_webview(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Kaset");
}

/// Picks up cookies the WebView rotated since the session was saved, then
/// opens the launch gate that API commands wait on.
/// `Session::refresh` ignores this unless the user is signed in.
fn refresh_cookies_from_webview(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        refresh_session_from_webview(&app, "main", "launch").await;
        app.state::<AppState>().launch.open();
    });
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;

    /// Regenerates `app/src/bindings.ts`. CI fails if the committed file differs.
    #[test]
    fn bindings_are_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");
        super::specta_builder()
            .mocks()
            .export(Typescript::default().header("// @ts-nocheck"), path)
            .expect("failed to export bindings");
    }
}
```

- [ ] **Step 7: Run the tests, all checks, and regenerate the bindings**

Run: `cargo fmt --all && cargo test -p kaset`
Expected: `test result: ok. 58 passed` (including the new `session::tests` barrier tests and `state::tests` launch-gate tests). `app/src/bindings.ts` changes only in the `getHome` doc comment (check with `git diff app/src/bindings.ts`).

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 8: Commit and push**

```bash
git add app/src-tauri/Cargo.toml app/src-tauri/src/session/mod.rs app/src-tauri/src/state.rs app/src-tauri/src/webview_session.rs app/src-tauri/src/login.rs app/src-tauri/src/commands.rs app/src-tauri/src/lib.rs app/src/bindings.ts
git commit -m "fix(app): add session epochs and move cookie work off async workers"
git push origin main
```

---

### Task 3: Player core — video ids, queue, and playback state machine

Pure Rust, no Tauri calls: everything the player decides is unit-tested here. `ytm-core`'s `Song` gains `Deserialize` so the UI can pass songs to `play_queue` (Task 4).

**Files:**
- Modify: `crates/ytm-core/src/models.rs`, `app/src-tauri/src/lib.rs` (one line)
- Create: `app/src-tauri/src/player/mod.rs`, `app/src-tauri/src/player/video_id.rs`, `app/src-tauri/src/player/queue.rs`, `app/src-tauri/src/player/engine.rs`
- Test: in each new file, and `crates/ytm-core/src/models.rs`

**Interfaces:**
- Consumes: `ytm_core::models::{Song, ArtistRef}`.
- Produces (all `pub`, under `kaset_lib::player`):
  - `video_id::VideoId` (`Clone, Eq, Hash, Serialize` transparent, custom `Deserialize` that validates, `specta::Type` = `string`): `VideoId::LEN = 11`, `parse(&str) -> Option<VideoId>`, `as_str(&self) -> &str`, `watch_url(&self) -> tauri::Url`.
  - `queue::RepeatMode { Off, All, One }` (serde `snake_case`, `Default = Off`, `specta::Type`), `queue::Advance { User, Ended }`, `queue::Queue`: `new(seed: u64)`, `replace(Vec<Song>, start: usize)`, `clear()`, `current() -> Option<&Song>`, `position() -> Option<usize>`, `ordered() -> Vec<Song>`, `repeat()`, `set_repeat(RepeatMode)`, `shuffle() -> bool`, `set_shuffle(bool)`, `has_next()`, `has_previous()`, `advance(Advance) -> Option<&Song>`, `retreat() -> Option<&Song>`.
  - `engine::{STATE_INTERVAL = 250 ms, PREVIOUS_RESTART_SECONDS = 3.0}`, `engine::PlaybackStatus { Idle, Loading, Playing, Paused, Ended }`, `engine::PlayerState { status, track: Option<Song>, position: f64, duration: f64, is_ad: bool, repeat: RepeatMode, shuffle: bool, has_previous: bool, has_next: bool }`, `engine::QueueState { songs: Vec<Song>, current: Option<u32> }`, `engine::PlaybackEvent::{Progress { video_id, playing, position, duration, ad }, Ended { video_id }, Failed { video_id }}` with `video_id(&self) -> &VideoId`, `engine::WebCommand::{Load(VideoId), Play, Pause, Seek(f64), Stop}`, `engine::Outcome { command: Option<WebCommand>, state: Option<PlayerState>, queue: Option<QueueState> }`, `engine::PlayerError::NothingToPlay`, `engine::Player`: `new(seed)`, `state()`, `queue_state()`, `current_video_id() -> Option<VideoId>`, `play_songs(Vec<Song>, start: usize, now: Instant) -> Result<Outcome, PlayerError>`, `play(now)`, `pause(now)`, `toggle(now)`, `next(now)`, `previous(now)`, `seek(f64, now)`, `set_repeat(RepeatMode, now)`, `set_shuffle(bool, now)`, `stop(now)`, `handle_event(PlaybackEvent, now)` — each returning `Outcome`.

- [ ] **Step 1: Write the failing tests**

Replace `crates/ytm-core/src/models.rs` with (adds `Deserialize` to `ArtistRef` and `Song` and a round-trip test):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArtistRef {
    pub name: String,
    pub browse_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowseCard {
    pub browse_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShelfItem {
    Song(Song),
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<ShelfItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomePage {
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}

#[cfg(all(test, feature = "specta"))]
mod specta_tests {
    use super::*;

    fn assert_type<T: specta::Type>() {}

    #[test]
    fn models_implement_specta_type() {
        assert_type::<ArtistRef>();
        assert_type::<Song>();
        assert_type::<BrowseCard>();
        assert_type::<ShelfItem>();
        assert_type::<HomeSection>();
        assert_type::<HomePage>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_round_trips_through_json() {
        let song = Song {
            video_id: "test-video1".into(),
            title: "Test song".into(),
            artists: vec![ArtistRef {
                name: "Test artist".into(),
                browse_id: Some("test-browse".into()),
            }],
            thumbnail_url: None,
        };
        let json = serde_json::to_string(&song).unwrap();
        assert_eq!(serde_json::from_str::<Song>(&json).unwrap(), song);
    }
}
```

Create `app/src-tauri/src/player/mod.rs`:

```rust
//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

pub mod engine;
pub mod queue;
pub mod video_id;
```

Add `pub mod player;` after `pub mod login;` in the module list at the top of `app/src-tauri/src/lib.rs`.

Create `app/src-tauri/src/player/video_id.rs` containing only this tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_eleven_url_safe_characters() {
        for id in ["dQw4w9WgXcQ", "abc-DEF_123", "___________", "-----------"] {
            assert_eq!(VideoId::parse(id).map(|id| id.0), Some(id.to_owned()));
        }
    }

    #[test]
    fn rejects_everything_else() {
        for id in [
            "",
            "short",
            "dQw4w9WgXcQx",
            "dQw4w9WgXc!",
            "dQw4w9WgXc ",
            "dQw4w9WgX&v",
            "dQw4w9WgX#a",
            "dQw4w9WgX/a",
            "dQw4w9WgXcé",
            "../../etc/p",
        ] {
            assert!(VideoId::parse(id).is_none(), "{id:?}");
        }
    }

    #[test]
    fn builds_the_watch_url() {
        let id = VideoId::parse("abc-DEF_123").unwrap();
        assert_eq!(
            id.watch_url().as_str(),
            "https://music.youtube.com/watch?v=abc-DEF_123"
        );
    }

    #[test]
    fn deserializing_validates() {
        assert_eq!(
            serde_json::from_str::<VideoId>("\"abc-DEF_123\"").unwrap(),
            VideoId::parse("abc-DEF_123").unwrap()
        );
        assert!(serde_json::from_str::<VideoId>("\"javascript:x\"").is_err());
        assert!(serde_json::from_str::<VideoId>("42").is_err());
    }
}
```

Create `app/src-tauri/src/player/queue.rs` containing only this tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song {
            video_id: id.into(),
            title: format!("Song {id}"),
            artists: Vec::new(),
            thumbnail_url: None,
        }
    }

    fn songs(count: usize) -> Vec<Song> {
        (0..count)
            .map(|index| song(&format!("test-vid-{index:02}")))
            .collect()
    }

    fn current_id(queue: &Queue) -> Option<&str> {
        queue.current().map(|song| song.video_id.as_str())
    }

    #[test]
    fn replace_starts_at_the_given_song() {
        let mut queue = Queue::new(1);
        queue.replace(songs(3), 1);
        assert_eq!(current_id(&queue), Some("test-vid-01"));
        assert_eq!(queue.position(), Some(1));
        assert_eq!(queue.ordered(), songs(3));
    }

    #[test]
    fn replace_with_an_invalid_start_empties_the_queue() {
        let mut queue = Queue::new(1);
        queue.replace(songs(3), 0);
        queue.replace(songs(3), 3);
        assert!(queue.current().is_none());
        assert!(queue.ordered().is_empty());
        queue.replace(Vec::new(), 0);
        assert!(queue.current().is_none());
    }

    #[test]
    fn advance_and_retreat_stop_at_the_ends_without_repeat() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 0);
        assert!(!queue.has_previous());
        assert!(queue.retreat().is_none());
        assert_eq!(current_id(&queue), Some("test-vid-00"));

        assert_eq!(
            queue
                .advance(Advance::User)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
        assert!(!queue.has_next());
        assert!(queue.advance(Advance::User).is_none());
        assert!(queue.advance(Advance::Ended).is_none());
        assert_eq!(current_id(&queue), Some("test-vid-01"));
        assert_eq!(
            queue.retreat().map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 1);
        queue.set_repeat(RepeatMode::All);
        assert!(queue.has_next());
        assert_eq!(
            queue
                .advance(Advance::Ended)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
        assert!(queue.has_previous());
        assert_eq!(
            queue.retreat().map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
    }

    #[test]
    fn repeat_one_replays_on_end_but_next_still_moves() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 0);
        queue.set_repeat(RepeatMode::One);
        assert_eq!(
            queue
                .advance(Advance::Ended)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
        assert_eq!(
            queue
                .advance(Advance::User)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
        assert!(queue.advance(Advance::User).is_none());
    }

    #[test]
    fn shuffle_keeps_the_current_song_first_and_permutes_the_rest() {
        let mut queue = Queue::new(42);
        queue.replace(songs(8), 3);
        queue.set_shuffle(true);
        assert_eq!(queue.position(), Some(0));
        assert_eq!(current_id(&queue), Some("test-vid-03"));
        let mut ids: Vec<String> = queue
            .ordered()
            .into_iter()
            .map(|song| song.video_id)
            .collect();
        assert_eq!(ids[0], "test-vid-03");
        ids.sort();
        let mut expected: Vec<String> = songs(8).into_iter().map(|song| song.video_id).collect();
        expected.sort();
        assert_eq!(ids, expected);
        assert_ne!(queue.ordered(), songs(8), "seed 42 changes the order");
    }

    #[test]
    fn shuffle_off_restores_order_and_keeps_the_current_song() {
        let mut queue = Queue::new(42);
        queue.replace(songs(8), 3);
        queue.set_shuffle(true);
        queue.advance(Advance::User);
        let playing = current_id(&queue).unwrap().to_owned();
        queue.set_shuffle(false);
        assert_eq!(queue.ordered(), songs(8));
        assert_eq!(current_id(&queue), Some(playing.as_str()));
    }

    #[test]
    fn replace_while_shuffled_plays_the_start_song_first() {
        let mut queue = Queue::new(7);
        queue.set_shuffle(true);
        queue.replace(songs(5), 2);
        assert_eq!(queue.position(), Some(0));
        assert_eq!(current_id(&queue), Some("test-vid-02"));
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let order = |seed| {
            let mut queue = Queue::new(seed);
            queue.replace(songs(10), 0);
            queue.set_shuffle(true);
            queue.ordered()
        };
        assert_eq!(order(5), order(5));
    }
}
```

Create `app/src-tauri/src/player/engine.rs` containing only this tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "test-vid-0A";
    const B: &str = "test-vid-0B";
    const C: &str = "test-vid-0C";

    fn song(id: &str) -> Song {
        Song {
            video_id: id.into(),
            title: format!("Song {id}"),
            artists: Vec::new(),
            thumbnail_url: None,
        }
    }

    fn id(value: &str) -> VideoId {
        VideoId::parse(value).unwrap()
    }

    fn progress(video: &str, playing: bool, position: f64) -> PlaybackEvent {
        PlaybackEvent::Progress {
            video_id: id(video),
            playing,
            position,
            duration: 200.0,
            ad: false,
        }
    }

    fn ended(video: &str) -> PlaybackEvent {
        PlaybackEvent::Ended {
            video_id: id(video),
        }
    }

    /// A player playing `songs[start]`, with the throttle window passed.
    fn playing(ids: &[&str], start: usize) -> (Player, Instant) {
        let mut player = Player::new(1);
        let now = Instant::now();
        player
            .play_songs(ids.iter().map(|value| song(value)).collect(), start, now)
            .unwrap();
        player.handle_event(progress(ids[start], true, 0.0), now);
        (player, now + STATE_INTERVAL)
    }

    #[test]
    fn play_songs_loads_the_start_song() {
        let mut player = Player::new(1);
        let outcome = player
            .play_songs(vec![song(A), song(B)], 1, Instant::now())
            .unwrap();
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        let state = outcome.state.unwrap();
        assert_eq!(state.status, PlaybackStatus::Loading);
        assert_eq!(state.track, Some(song(B)));
        assert_eq!(outcome.queue.unwrap().current, Some(1));
    }

    #[test]
    fn play_songs_drops_invalid_ids_and_keeps_the_start_song() {
        let mut player = Player::new(1);
        let outcome = player
            .play_songs(
                vec![song("bad id"), song(A), song("x"), song(B)],
                3,
                Instant::now(),
            )
            .unwrap();
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        let queue = outcome.queue.unwrap();
        assert_eq!(queue.songs, vec![song(A), song(B)]);
        assert_eq!(queue.current, Some(1));
    }

    #[test]
    fn play_songs_rejects_an_invalid_start() {
        let mut player = Player::new(1);
        let now = Instant::now();
        assert_eq!(
            player.play_songs(vec![song(A)], 1, now),
            Err(PlayerError::NothingToPlay)
        );
        assert_eq!(
            player.play_songs(vec![song("javascript:")], 0, now),
            Err(PlayerError::NothingToPlay)
        );
        assert_eq!(player.state().status, PlaybackStatus::Idle);
    }

    #[test]
    fn progress_for_another_video_is_ignored() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.handle_event(progress(C, false, 50.0), now);
        assert_eq!(outcome, Outcome::default());
        assert_eq!(player.state().status, PlaybackStatus::Playing);
        assert_eq!(player.state().position, 0.0);
    }

    #[test]
    fn loading_stays_loading_until_the_page_plays() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player.play_songs(vec![song(A)], 0, now).unwrap();
        player.handle_event(progress(A, false, 0.0), now + STATE_INTERVAL);
        assert_eq!(player.state().status, PlaybackStatus::Loading);
        let outcome = player.handle_event(progress(A, true, 0.1), now + STATE_INTERVAL);
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Playing);
    }

    #[test]
    fn progress_is_throttled_but_status_changes_are_not() {
        let (mut player, now) = playing(&[A], 0);
        assert!(
            player
                .handle_event(progress(A, true, 1.0), now)
                .state
                .is_some()
        );
        let soon = now + Duration::from_millis(100);
        assert!(
            player
                .handle_event(progress(A, true, 1.1), soon)
                .state
                .is_none()
        );
        assert_eq!(player.state().position, 1.1, "state still updates");
        let paused = player.handle_event(progress(A, false, 1.2), soon);
        assert_eq!(paused.state.unwrap().status, PlaybackStatus::Paused);
        let later = soon + STATE_INTERVAL;
        assert!(
            player
                .handle_event(progress(A, false, 1.2), later)
                .state
                .is_some()
        );
    }

    #[test]
    fn ended_loads_the_next_song_once() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        assert_eq!(outcome.queue.unwrap().current, Some(1));
        // A late duplicate for A, and a duplicate for B while it loads.
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
        assert_eq!(player.handle_event(ended(B), now), Outcome::default());
    }

    #[test]
    fn ended_on_the_last_song_holds_the_page() {
        let (mut player, now) = playing(&[A], 0);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Pause));
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Ended);
        // Paused progress after the end keeps the ended state.
        player.handle_event(progress(A, false, 200.0), now + STATE_INTERVAL);
        assert_eq!(player.state().status, PlaybackStatus::Ended);
        // Play starts the song again.
        let again = player.play(now + STATE_INTERVAL);
        assert_eq!(again.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_with_repeat_one_reloads_the_same_song() {
        let (mut player, now) = playing(&[A, B], 0);
        player.set_repeat(RepeatMode::One, now);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_with_repeat_all_wraps() {
        let (mut player, now) = playing(&[A, B], 1);
        player.set_repeat(RepeatMode::All, now);
        let outcome = player.handle_event(ended(B), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_during_an_ad_is_ignored() {
        let (mut player, now) = playing(&[A, B], 0);
        player.handle_event(
            PlaybackEvent::Progress {
                video_id: id(A),
                playing: true,
                position: 3.0,
                duration: 15.0,
                ad: true,
            },
            now,
        );
        assert!(player.state().is_ad);
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
        assert_eq!(
            player.seek(10.0, now),
            Outcome::default(),
            "no seeking during ads"
        );
    }

    #[test]
    fn toggle_pauses_and_resumes() {
        let (mut player, now) = playing(&[A], 0);
        let paused = player.toggle(now);
        assert_eq!(paused.command, Some(WebCommand::Pause));
        assert_eq!(player.state().status, PlaybackStatus::Paused);
        let resumed = player.toggle(now);
        assert_eq!(resumed.command, Some(WebCommand::Play));
        assert_eq!(player.state().status, PlaybackStatus::Playing);
    }

    #[test]
    fn controls_do_nothing_when_idle() {
        let mut player = Player::new(1);
        let now = Instant::now();
        assert_eq!(player.toggle(now), Outcome::default());
        assert_eq!(player.next(now), Outcome::default());
        assert_eq!(player.previous(now), Outcome::default());
        assert_eq!(player.seek(5.0, now), Outcome::default());
    }

    #[test]
    fn next_moves_and_stops_at_the_end() {
        let (mut player, now) = playing(&[A, B], 0);
        assert_eq!(player.next(now).command, Some(WebCommand::Load(id(B))));
        player.handle_event(progress(B, true, 0.0), now);
        assert_eq!(player.next(now), Outcome::default());
    }

    #[test]
    fn previous_restarts_after_three_seconds_and_otherwise_goes_back() {
        let (mut player, now) = playing(&[A, B], 1);
        player.handle_event(progress(B, true, 10.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Seek(0.0)));
        player.handle_event(progress(B, true, 1.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Load(id(A))));
        // At the start of the queue previous restarts.
        player.handle_event(progress(A, true, 1.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Seek(0.0)));
    }

    #[test]
    fn seek_clamps_to_the_duration() {
        let (mut player, now) = playing(&[A], 0);
        player.handle_event(progress(A, true, 1.0), now);
        assert_eq!(
            player.seek(500.0, now).command,
            Some(WebCommand::Seek(200.0))
        );
        assert_eq!(player.seek(-3.0, now).command, Some(WebCommand::Seek(0.0)));
        assert_eq!(
            player.seek(f64::NAN, now).command,
            Some(WebCommand::Seek(0.0))
        );
    }

    #[test]
    fn non_finite_progress_is_sanitized() {
        let (mut player, now) = playing(&[A], 0);
        player.handle_event(
            PlaybackEvent::Progress {
                video_id: id(A),
                playing: true,
                position: f64::INFINITY,
                duration: f64::NAN,
                ad: false,
            },
            now,
        );
        assert_eq!(
            (player.state().position, player.state().duration),
            (0.0, 0.0)
        );
    }

    #[test]
    fn shuffle_emits_the_new_queue_order() {
        let (mut player, now) = playing(&[A, B, C], 0);
        let outcome = player.set_shuffle(true, now);
        let queue = outcome.queue.unwrap();
        assert_eq!(queue.current, Some(0));
        assert_eq!(queue.songs[0], song(A));
        assert!(outcome.state.unwrap().shuffle);
    }

    #[test]
    fn failed_pauses() {
        let (mut player, now) = playing(&[A], 0);
        let outcome = player.handle_event(PlaybackEvent::Failed { video_id: id(A) }, now);
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Paused);
    }

    #[test]
    fn stop_empties_the_queue_and_closes_the_page() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.stop(now);
        assert_eq!(outcome.command, Some(WebCommand::Stop));
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Idle);
        assert_eq!(outcome.queue.unwrap().songs, Vec::<Song>::new());
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
    }

    #[test]
    fn events_deserialize_from_the_init_script_shape() {
        let event: PlaybackEvent = serde_json::from_str(
            r#"{"kind":"progress","video_id":"test-vid-0A","playing":true,"position":1.5,"duration":200,"ad":false}"#,
        )
        .unwrap();
        assert_eq!(event, progress(A, true, 1.5));
        let event: PlaybackEvent =
            serde_json::from_str(r#"{"kind":"ended","video_id":"test-vid-0A"}"#).unwrap();
        assert_eq!(event, ended(A));
    }

    #[test]
    fn events_with_bad_ids_or_extra_fields_are_rejected() {
        for json in [
            r#"{"kind":"ended","video_id":"not-a-valid-id"}"#,
            r#"{"kind":"ended","video_id":"test-vid-0A","url":"https://example.com"}"#,
            r#"{"kind":"navigate","video_id":"test-vid-0A"}"#,
            r#"{"kind":"progress","video_id":"test-vid-0A","playing":"yes","position":1,"duration":2,"ad":false}"#,
        ] {
            assert!(
                serde_json::from_str::<PlaybackEvent>(json).is_err(),
                "{json}"
            );
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset player::`
Expected: compile errors such as `cannot find type 'VideoId' in this scope`, `cannot find type 'Queue'`, `cannot find type 'Player'`.

- [ ] **Step 3: Implement `VideoId`**

Insert above the tests module in `app/src-tauri/src/player/video_id.rs`:

```rust
use serde::{Deserialize, Deserializer, Serialize};
use tauri::Url;

/// A YouTube video id: exactly 11 characters from `[A-Za-z0-9_-]`. Checked
/// before any id reaches a URL or the player, whether it came from the UI,
/// the API, or the player page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(transparent)]
pub struct VideoId(String);

impl VideoId {
    pub const LEN: usize = 11;

    pub fn parse(value: &str) -> Option<Self> {
        let valid = value.len() == Self::LEN
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid.then(|| Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `https://music.youtube.com/watch?v=<id>`.
    pub fn watch_url(&self) -> Url {
        let mut url = Url::parse("https://music.youtube.com/watch").expect("constant URL is valid");
        url.query_pairs_mut().append_pair("v", &self.0);
        url
    }
}

impl<'de> Deserialize<'de> for VideoId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("invalid video id"))
    }
}
```

- [ ] **Step 4: Implement the queue**

Insert above the tests module in `app/src-tauri/src/player/queue.rs`:

```rust
//! The play queue. Pure Rust: no Tauri types, deterministic for a given seed.

use serde::{Deserialize, Serialize};
use ytm_core::models::Song;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    /// Wrap from the last song to the first.
    All,
    /// Replay the current song when it ends. Next and previous still move.
    One,
}

/// Why the queue moves forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advance {
    /// Next button, media key.
    User,
    /// The current song finished playing.
    Ended,
}

/// xorshift64: enough to shuffle a queue, no dependency.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn below(&mut self, bound: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % bound as u64) as usize
    }
}

#[derive(Debug, Clone)]
pub struct Queue {
    /// Songs in the order they were given.
    songs: Vec<Song>,
    /// Play order: indices into `songs`. The identity unless shuffled.
    order: Vec<usize>,
    /// Index into `order` of the current song.
    position: Option<usize>,
    repeat: RepeatMode,
    shuffle: bool,
    rng: Rng,
}

impl Queue {
    pub fn new(seed: u64) -> Self {
        Self {
            songs: Vec::new(),
            order: Vec::new(),
            position: None,
            repeat: RepeatMode::Off,
            shuffle: false,
            rng: Rng::new(seed),
        }
    }

    /// Replaces the queue and makes `songs[start]` current. With shuffle on,
    /// the start song plays first and the rest are shuffled. An out-of-range
    /// `start` empties the queue.
    pub fn replace(&mut self, songs: Vec<Song>, start: usize) {
        if start >= songs.len() {
            self.clear();
            return;
        }
        self.songs = songs;
        self.order = (0..self.songs.len()).collect();
        self.position = Some(start);
        if self.shuffle {
            self.shuffle_around_current();
        }
    }

    pub fn clear(&mut self) {
        self.songs.clear();
        self.order.clear();
        self.position = None;
    }

    pub fn current(&self) -> Option<&Song> {
        self.position
            .map(|position| &self.songs[self.order[position]])
    }

    /// Index of the current song in play order.
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// Songs in play order.
    pub fn ordered(&self) -> Vec<Song> {
        self.order
            .iter()
            .map(|&index| self.songs[index].clone())
            .collect()
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Turning shuffle on keeps the current song and shuffles the others
    /// after it; turning it off restores the original order.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        if shuffle == self.shuffle {
            return;
        }
        self.shuffle = shuffle;
        if shuffle {
            self.shuffle_around_current();
        } else {
            let current = self.position.map(|position| self.order[position]);
            self.order = (0..self.songs.len()).collect();
            self.position = current;
        }
    }

    /// Moves `order` so the current song comes first and shuffles the rest.
    fn shuffle_around_current(&mut self) {
        let Some(position) = self.position else {
            return;
        };
        let current = self.order.remove(position);
        let mut rest = std::mem::take(&mut self.order);
        // Fisher–Yates.
        for index in (1..rest.len()).rev() {
            let other = self.rng.below(index + 1);
            rest.swap(index, other);
        }
        self.order = std::iter::once(current).chain(rest).collect();
        self.position = Some(0);
    }

    pub fn has_next(&self) -> bool {
        self.position.is_some_and(|position| {
            position + 1 < self.order.len() || self.repeat == RepeatMode::All
        })
    }

    pub fn has_previous(&self) -> bool {
        self.position
            .is_some_and(|position| position > 0 || self.repeat == RepeatMode::All)
    }

    /// Moves to the next song and returns it. `Ended` with `RepeatMode::One`
    /// stays on the current song. At the end of the queue this wraps with
    /// `RepeatMode::All` and otherwise returns `None` without moving.
    pub fn advance(&mut self, reason: Advance) -> Option<&Song> {
        let position = self.position?;
        if reason == Advance::Ended && self.repeat == RepeatMode::One {
            return self.current();
        }
        let next = if position + 1 < self.order.len() {
            position + 1
        } else if self.repeat == RepeatMode::All {
            0
        } else {
            return None;
        };
        self.position = Some(next);
        self.current()
    }

    /// Moves to the previous song and returns it. At the start this wraps with
    /// `RepeatMode::All` and otherwise returns `None` without moving.
    pub fn retreat(&mut self) -> Option<&Song> {
        let position = self.position?;
        let previous = if position > 0 {
            position - 1
        } else if self.repeat == RepeatMode::All {
            self.order.len() - 1
        } else {
            return None;
        };
        self.position = Some(previous);
        self.current()
    }
}
```

- [ ] **Step 5: Implement the engine**

Insert above the tests module in `app/src-tauri/src/player/engine.rs`:

```rust
//! Player state machine. Pure Rust: commands and player-page events go in,
//! an `Outcome` (what to tell the player page, what to emit) comes out. The
//! Tauri glue in `service.rs` applies outcomes.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ytm_core::models::Song;

use super::queue::{Advance, Queue, RepeatMode};
use super::video_id::VideoId;

/// `player://state` is emitted at most this often for progress-only changes.
pub const STATE_INTERVAL: Duration = Duration::from_millis(250);
/// Previous restarts the current song when it has played longer than this.
pub const PREVIOUS_RESTART_SECONDS: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    /// Nothing queued.
    Idle,
    /// The player page is loading the current song.
    Loading,
    Playing,
    Paused,
    /// The last song finished and nothing follows it.
    Ended,
}

/// Emitted on `player://state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub track: Option<Song>,
    /// Seconds.
    pub position: f64,
    /// Seconds; 0 while unknown.
    pub duration: f64,
    /// An ad is playing; position and duration belong to the ad.
    pub is_ad: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub has_previous: bool,
    pub has_next: bool,
}

/// Emitted on `player://queue`: songs in play order and the current index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct QueueState {
    pub songs: Vec<Song>,
    pub current: Option<u32>,
}

/// Sent by the init script in the player window through `playback_event`.
/// Fixed-size fields only: no free-form strings, lists, or maps.
#[derive(Debug, Clone, PartialEq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaybackEvent {
    /// Sampled on media events and about four times a second while playing.
    Progress {
        video_id: VideoId,
        playing: bool,
        position: f64,
        duration: f64,
        ad: bool,
    },
    /// The song's media ended (never sent for an ad).
    Ended { video_id: VideoId },
    /// The page could not start or continue playback.
    Failed { video_id: VideoId },
}

impl PlaybackEvent {
    pub fn video_id(&self) -> &VideoId {
        match self {
            Self::Progress { video_id, .. }
            | Self::Ended { video_id }
            | Self::Failed { video_id } => video_id,
        }
    }
}

/// What the player page should do.
#[derive(Debug, Clone, PartialEq)]
pub enum WebCommand {
    /// Navigate the player window to the song's watch page; it starts playing.
    Load(VideoId),
    Play,
    Pause,
    /// Seconds.
    Seek(f64),
    /// Close the player window.
    Stop,
}

/// The result of one command or event.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub command: Option<WebCommand>,
    /// Emit on `player://state` (already throttled).
    pub state: Option<PlayerState>,
    /// Emit on `player://queue`.
    pub queue: Option<QueueState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PlayerError {
    #[error("nothing playable at that position")]
    NothingToPlay,
}

fn seconds(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub struct Player {
    queue: Queue,
    status: PlaybackStatus,
    position: f64,
    duration: f64,
    is_ad: bool,
    last_emit: Option<Instant>,
}

impl Player {
    pub fn new(seed: u64) -> Self {
        Self {
            queue: Queue::new(seed),
            status: PlaybackStatus::Idle,
            position: 0.0,
            duration: 0.0,
            is_ad: false,
            last_emit: None,
        }
    }

    pub fn state(&self) -> PlayerState {
        PlayerState {
            status: self.status,
            track: self.queue.current().cloned(),
            position: self.position,
            duration: self.duration,
            is_ad: self.is_ad,
            repeat: self.queue.repeat(),
            shuffle: self.queue.shuffle(),
            has_previous: self.queue.has_previous(),
            has_next: self.queue.has_next(),
        }
    }

    pub fn queue_state(&self) -> QueueState {
        QueueState {
            songs: self.queue.ordered(),
            current: self
                .queue
                .position()
                .and_then(|position| u32::try_from(position).ok()),
        }
    }

    /// The song the player page should be playing.
    pub fn current_video_id(&self) -> Option<VideoId> {
        self.queue
            .current()
            .and_then(|song| VideoId::parse(&song.video_id))
    }

    /// State to emit: always when `force`, otherwise at most every `STATE_INTERVAL`.
    fn emit(&mut self, now: Instant, force: bool) -> Option<PlayerState> {
        let due = self
            .last_emit
            .is_none_or(|last| now.saturating_duration_since(last) >= STATE_INTERVAL);
        (force || due).then(|| {
            self.last_emit = Some(now);
            self.state()
        })
    }

    /// Starts loading the current song.
    fn load_current(&mut self, now: Instant) -> Outcome {
        let Some(id) = self.current_video_id() else {
            return self.stop(now);
        };
        self.status = PlaybackStatus::Loading;
        self.position = 0.0;
        self.duration = 0.0;
        self.is_ad = false;
        Outcome {
            command: Some(WebCommand::Load(id)),
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Replaces the queue with `songs` and plays `songs[start]`. Songs whose
    /// video id is invalid are dropped.
    pub fn play_songs(
        &mut self,
        songs: Vec<Song>,
        start: usize,
        now: Instant,
    ) -> Result<Outcome, PlayerError> {
        let start_valid = songs
            .get(start)
            .is_some_and(|song| VideoId::parse(&song.video_id).is_some());
        if !start_valid {
            return Err(PlayerError::NothingToPlay);
        }
        let skipped_before = songs[..start]
            .iter()
            .filter(|song| VideoId::parse(&song.video_id).is_none())
            .count();
        let playable: Vec<Song> = songs
            .into_iter()
            .filter(|song| VideoId::parse(&song.video_id).is_some())
            .collect();
        self.queue.replace(playable, start - skipped_before);
        Ok(self.load_current(now))
    }

    pub fn play(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Idle | PlaybackStatus::Playing => Outcome::default(),
            // The page holds playback after the last song; start it again.
            PlaybackStatus::Ended => self.load_current(now),
            PlaybackStatus::Loading | PlaybackStatus::Paused => {
                self.status = PlaybackStatus::Playing;
                Outcome {
                    command: Some(WebCommand::Play),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
        }
    }

    pub fn pause(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Loading | PlaybackStatus::Playing => {
                self.status = PlaybackStatus::Paused;
                Outcome {
                    command: Some(WebCommand::Pause),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
            _ => Outcome::default(),
        }
    }

    pub fn toggle(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Loading | PlaybackStatus::Playing => self.pause(now),
            _ => self.play(now),
        }
    }

    pub fn next(&mut self, now: Instant) -> Outcome {
        if self.queue.advance(Advance::User).is_some() {
            self.load_current(now)
        } else {
            Outcome::default()
        }
    }

    /// Restarts the song after `PREVIOUS_RESTART_SECONDS` or at the start of
    /// the queue; otherwise moves to the previous song.
    pub fn previous(&mut self, now: Instant) -> Outcome {
        if self.queue.current().is_none() {
            return Outcome::default();
        }
        if self.position > PREVIOUS_RESTART_SECONDS || !self.queue.has_previous() {
            return self.seek(0.0, now);
        }
        self.queue.retreat();
        self.load_current(now)
    }

    /// Ignored while an ad plays.
    pub fn seek(&mut self, position: f64, now: Instant) -> Outcome {
        if self.queue.current().is_none() || self.is_ad {
            return Outcome::default();
        }
        let mut position = seconds(position);
        if self.duration > 0.0 {
            position = position.min(self.duration);
        }
        self.position = position;
        Outcome {
            command: Some(WebCommand::Seek(position)),
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode, now: Instant) -> Outcome {
        self.queue.set_repeat(repeat);
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_shuffle(&mut self, shuffle: bool, now: Instant) -> Outcome {
        self.queue.set_shuffle(shuffle);
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Empties the queue and closes the player window (sign-out).
    pub fn stop(&mut self, now: Instant) -> Outcome {
        self.queue.clear();
        self.status = PlaybackStatus::Idle;
        self.position = 0.0;
        self.duration = 0.0;
        self.is_ad = false;
        Outcome {
            command: Some(WebCommand::Stop),
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Applies an event from the player page. Events for any video other than
    /// the current song are ignored (the page's own autoplay, a late event
    /// from the previous song, or a forged event).
    pub fn handle_event(&mut self, event: PlaybackEvent, now: Instant) -> Outcome {
        if self.current_video_id().as_ref() != Some(event.video_id()) {
            return Outcome::default();
        }
        match event {
            PlaybackEvent::Progress {
                playing,
                position,
                duration,
                ad,
                ..
            } => {
                let status = match (self.status, playing) {
                    (_, true) => PlaybackStatus::Playing,
                    // Still waiting for the page to start the song.
                    (PlaybackStatus::Loading, false) => PlaybackStatus::Loading,
                    (PlaybackStatus::Ended, false) => PlaybackStatus::Ended,
                    (_, false) => PlaybackStatus::Paused,
                };
                let force = status != self.status || ad != self.is_ad;
                self.status = status;
                self.position = seconds(position);
                self.duration = seconds(duration);
                self.is_ad = ad;
                Outcome {
                    command: None,
                    state: self.emit(now, force),
                    queue: None,
                }
            }
            PlaybackEvent::Ended { .. } => {
                // A duplicate ended for a song already advanced from, or an ad.
                if matches!(self.status, PlaybackStatus::Loading | PlaybackStatus::Ended)
                    || self.is_ad
                {
                    return Outcome::default();
                }
                if self.queue.advance(Advance::Ended).is_some() {
                    return self.load_current(now);
                }
                self.status = PlaybackStatus::Ended;
                self.position = self.duration;
                Outcome {
                    command: Some(WebCommand::Pause),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
            PlaybackEvent::Failed { .. } => {
                if self.status == PlaybackStatus::Ended {
                    return Outcome::default();
                }
                self.status = PlaybackStatus::Paused;
                Outcome {
                    command: None,
                    state: self.emit(now, true),
                    queue: None,
                }
            }
        }
    }
}
```

- [ ] **Step 6: Run the tests and checks**

Run: `cargo test -p kaset player:: && cargo test -p ytm-core`
Expected: `test result: ok. 35 passed` for `player::`; ytm-core's unit tests include `song_round_trips_through_json` and pass.

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p ytm-core --all-targets -- -D warnings && cargo test --workspace`
Expected: all pass (`kaset` lib: 93 tests). `app/src/bindings.ts` is unchanged (`git diff --exit-code app/src/bindings.ts`).

- [ ] **Step 7: Commit and push**

```bash
git add crates/ytm-core/src/models.rs app/src-tauri/src/lib.rs app/src-tauri/src/player/mod.rs app/src-tauri/src/player/video_id.rs app/src-tauri/src/player/queue.rs app/src-tauri/src/player/engine.rs
git commit -m "feat(app): add player queue and playback state machine"
git push origin main
```

---

### Task 4: Player bridge — hidden window, init script, capability, commands, events

Wires the engine to Tauri. The hidden `player` window loads `watch?v=<id>`; the init script reports `<video>` state through `playback_event`; `PlayerService` applies engine outcomes (navigate / eval / close, emit events). Sign-out stops playback; closing the main window quits the app.

**Files:**
- Create: `app/src-tauri/src/player/player.js`, `app/src-tauri/src/player/window.rs`, `app/src-tauri/src/player/service.rs`, `app/src-tauri/src/player/commands.rs`, `app/src-tauri/capabilities/player.json`
- Modify: `app/src-tauri/src/player/mod.rs`, `app/src-tauri/src/events.rs`, `app/src-tauri/src/error.rs`, `app/src-tauri/src/state.rs`, `app/src-tauri/src/commands.rs`, `app/src-tauri/src/session/cookies.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/build.rs`, `app/src-tauri/capabilities/default.json`
- Generated (commit them): `app/src-tauri/permissions/autogenerated/{play_queue,toggle_playback,next_track,previous_track,seek_to,set_repeat,set_shuffle,player_snapshot,playback_event}.toml`, `app/src/bindings.ts`
- Test: `app/src-tauri/src/player/window.rs`

**Interfaces:**
- Consumes: Task 2 `blocking`, `refresh_session_from_webview`, `AppState`; Task 3 `Player`, `Outcome`, `WebCommand`, `PlayerState`, `QueueState`, `PlaybackEvent`, `RepeatMode`, `VideoId`; `crate::login::SAFARI_USER_AGENT` (macOS); `crate::session::cookies::{is_country_google, is_under}`.
- Produces:
  - `player::window::{PLAYER_LABEL = "player", PLAYER_SCRIPT, is_allowed_player_navigation(&Url) -> bool, is_music_page(&Url) -> bool, seek_script(f64) -> String, play_script() -> &'static str, pause_script() -> &'static str, load(&AppHandle, &VideoId) -> Result<(), AppError>, run_script(&AppHandle, &str) -> Result<(), AppError>, close(&AppHandle)}`.
  - `player::service::PlayerService`: `new()`, `async fn update<F>(&self, app: &AppHandle, change: F) -> Result<(), AppError> where F: FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>`, `async fn snapshot(&self) -> (PlayerState, QueueState)`. `AppState` gains `pub player: PlayerService`.
  - `events::PlayerStateChanged { state: PlayerState }` (`player://state`), `events::QueueChanged { queue: QueueState }` (`player://queue`). TS: `events.playerState`, `events.playerQueue`.
  - `ErrorKind::InvalidArgument` (`"invalid_argument"`).
  - Commands (TS in parentheses), all rejecting with `AppError`: `play_queue(songs: Vec<Song>, start_index: u32)` (`commands.playQueue(songs, startIndex)`), `toggle_playback` (`togglePlayback()`), `next_track` (`nextTrack()`), `previous_track` (`previousTrack()`), `seek_to(position: f64)` (`seekTo(position)`), `set_repeat(mode: RepeatMode)` (`setRepeat(mode)`), `set_shuffle(enabled: bool)` (`setShuffle(enabled)`), `player_snapshot() -> PlayerSnapshot { state, queue }` (`playerSnapshot()`), `playback_event(event: PlaybackEvent)` (player window only). `play_queue` rejects with `not_signed_in` unless signed in and `invalid_argument` for more than 500 songs or an unplayable start.
  - JS → Rust payload: `__TAURI_INTERNALS__.invoke('playback_event', { event: { kind, video_id, … } })`.

- [ ] **Step 1: Write the failing window tests**

Create `app/src-tauri/src/player/window.rs` containing only this tests module:

```rust
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
```

Replace `app/src-tauri/src/player/mod.rs` with:

```rust
//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

pub mod engine;
pub mod queue;
pub mod video_id;
pub mod window;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset player::window`
Expected: compile errors such as `cannot find function 'is_allowed_player_navigation'`, `cannot find value 'PLAYER_SCRIPT'`.

- [ ] **Step 3: Write the init script and the window module**

In `app/src-tauri/src/session/cookies.rs`, make the domain helper crate-visible: change `fn is_under(domain: &str, root: &str) -> bool {` to `pub(crate) fn is_under(domain: &str, root: &str) -> bool {`.

Create `app/src-tauri/src/player/player.js`:

```javascript
// Kaset player bridge. Runs in the hidden player window before page scripts,
// in the main frame only. Reports the <video> element's state to Rust through
// the `playback_event` command, the only command this origin may call, and
// exposes play/pause/seek for Rust to call with `eval`.
(() => {
  'use strict';
  if (window.top !== window || location.origin !== 'https://music.youtube.com') return;
  if (window.__kasetPlayer) return;

  const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/;
  const MAX_AUTOPLAY_ATTEMPTS = 5;
  const AUTOPLAY_RETRY_MS = 500;
  const PROGRESS_INTERVAL_MS = 250;

  // Rust navigates to watch?v=<id> for every song, so the URL names the song
  // this document was loaded for. The page's own autoplay changes it, and Rust
  // ignores events for any other id.
  function currentVideoId() {
    try {
      const id = new URL(location.href).searchParams.get('v') || '';
      return VIDEO_ID.test(id) ? id : null;
    } catch (_) {
      return null;
    }
  }

  function send(event) {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') return;
    Promise.resolve(internals.invoke('playback_event', { event })).catch(() => {});
  }

  // Minimal ad detection, after PlaybackAdDetectionScript in the Swift app.
  function isAd() {
    const moviePlayer = document.getElementById('movie_player');
    if (moviePlayer && moviePlayer.classList
      && (moviePlayer.classList.contains('ad-showing')
        || moviePlayer.classList.contains('ad-interrupting'))) {
      return true;
    }
    const musicPlayer = document.querySelector('ytmusic-player');
    for (const api of [moviePlayer, musicPlayer && musicPlayer.playerApi]) {
      try {
        // Player type 2 is ad media.
        if (api && typeof api.getPresentingPlayerType === 'function'
          && api.getPresentingPlayerType(true) === 2) {
          return true;
        }
      } catch (_) {}
    }
    return false;
  }

  function seconds(value) {
    return Number.isFinite(value) && value >= 0 ? value : 0;
  }

  // `hold` blocks playback after a pause command or after the song ended, so
  // the page's own autoplay cannot start a song Kaset did not ask for.
  let hold = false;
  // A fresh document starts its song; YouTube Music's page does not.
  let autoplayPending = true;
  let autoplayAttempts = 0;
  let hookedVideo = null;
  let lastProgressAt = 0;

  function reportProgress(video, force) {
    const now = Date.now();
    if (!force && now - lastProgressAt < PROGRESS_INTERVAL_MS) return;
    const id = currentVideoId();
    if (!id) return;
    lastProgressAt = now;
    send({
      kind: 'progress',
      video_id: id,
      playing: !video.paused && !video.ended,
      position: seconds(video.currentTime),
      duration: seconds(video.duration),
      ad: isAd(),
    });
  }

  function reportFailure() {
    const id = currentVideoId();
    if (id) send({ kind: 'failed', video_id: id });
  }

  // Like the Swift app's autoplay recovery: call video.play() and retry until
  // the element plays or the attempts run out.
  function tryAutoplay(video) {
    if (video !== document.querySelector('video')) return;
    if (!video.paused) {
      autoplayPending = false;
      autoplayAttempts = 0;
      return;
    }
    if (hold || !autoplayPending) return;
    if (autoplayAttempts >= MAX_AUTOPLAY_ATTEMPTS) {
      autoplayPending = false;
      reportFailure();
      return;
    }
    autoplayAttempts += 1;
    Promise.resolve()
      .then(() => video.play())
      .catch(() => {});
    setTimeout(() => tryAutoplay(video), AUTOPLAY_RETRY_MS);
  }

  function hook(video) {
    hookedVideo = video;
    video.addEventListener('play', () => {
      if (hold) {
        video.pause();
        return;
      }
      reportProgress(video, true);
    });
    video.addEventListener('playing', () => reportProgress(video, true));
    video.addEventListener('pause', () => reportProgress(video, true));
    video.addEventListener('seeked', () => reportProgress(video, true));
    video.addEventListener('timeupdate', () => reportProgress(video, false));
    video.addEventListener('loadedmetadata', () => {
      reportProgress(video, true);
      tryAutoplay(video);
    });
    video.addEventListener('canplay', () => tryAutoplay(video));
    video.addEventListener('ended', () => {
      if (isAd()) return;
      const id = currentVideoId();
      if (!id) return;
      hold = true;
      send({ kind: 'ended', video_id: id });
    });
    video.addEventListener('error', reportFailure);
    if (video.readyState >= 1) tryAutoplay(video);
  }

  // The page creates and may replace its <video> element after load.
  setInterval(() => {
    const video = document.querySelector('video');
    if (video && video !== hookedVideo) hook(video);
  }, 500);

  Object.defineProperty(window, '__kasetPlayer', {
    value: Object.freeze({
      play() {
        hold = false;
        autoplayPending = true;
        autoplayAttempts = 0;
        const video = document.querySelector('video');
        if (video) tryAutoplay(video);
      },
      pause() {
        hold = true;
        autoplayPending = false;
        const video = document.querySelector('video');
        if (video && !video.paused) video.pause();
      },
      seek(position) {
        const video = document.querySelector('video');
        if (!video || !Number.isFinite(position)) return;
        video.currentTime = Math.max(0, position);
        reportProgress(video, true);
      },
    }),
    writable: false,
    configurable: false,
  });
})();
```

Insert above the tests module in `app/src-tauri/src/player/window.rs`:

```rust
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
```

Run: `node --check app/src-tauri/src/player/player.js && cargo test -p kaset player::window`
Expected: no syntax error, then `test result: ok. 5 passed`.

- [ ] **Step 4: Add the service, commands, events, and error kind**

Create `app/src-tauri/src/player/service.rs`:

```rust
//! Applies engine outcomes: drives the player window, emits `player://state`
//! and `player://queue` to the main window, and updates the OS media controls.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::Mutex;

use super::engine::{Outcome, Player, PlayerState, QueueState, WebCommand};
use super::window;
use crate::error::AppError;
use crate::events::{PlayerStateChanged, QueueChanged};

/// Player events go to the main window. The player window's capability has
/// no event permissions, so its pages cannot listen to them either.
const UI_WINDOW: &str = "main";

pub struct PlayerService {
    // A tokio mutex, held while an outcome is applied, so window commands run
    // in the order the engine produced them. Never locked on the main thread.
    player: Mutex<Player>,
}

impl PlayerService {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1);
        Self {
            player: Mutex::new(Player::new(seed)),
        }
    }

    /// Runs `change` on the engine and applies its outcome.
    pub async fn update<F>(&self, app: &AppHandle, change: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>,
    {
        let mut player = self.player.lock().await;
        let outcome = change(&mut player, Instant::now())?;
        apply(app, outcome)
    }

    pub async fn snapshot(&self) -> (PlayerState, QueueState) {
        let player = self.player.lock().await;
        (player.state(), player.queue_state())
    }
}

impl Default for PlayerService {
    fn default() -> Self {
        Self::new()
    }
}

fn apply(app: &AppHandle, outcome: Outcome) -> Result<(), AppError> {
    let result = match outcome.command {
        Some(WebCommand::Load(id)) => window::load(app, &id),
        Some(WebCommand::Play) => window::run_script(app, window::play_script()),
        Some(WebCommand::Pause) => window::run_script(app, window::pause_script()),
        Some(WebCommand::Seek(position)) => window::run_script(app, &window::seek_script(position)),
        Some(WebCommand::Stop) => {
            window::close(app);
            Ok(())
        }
        None => Ok(()),
    };
    if let Some(queue) = outcome.queue
        && let Err(error) = (QueueChanged { queue }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://queue");
    }
    if let Some(state) = outcome.state
        && let Err(error) = (PlayerStateChanged { state }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://state");
    }
    result
}
```

Create `app/src-tauri/src/player/commands.rs`:

```rust
//! Player commands. The UI calls all but `playback_event`, which only the
//! player window's music.youtube.com pages may call (capability `player-remote`).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, Webview};
use ytm_core::models::Song;

use super::engine::{PlaybackEvent, PlayerState, QueueState};
use super::queue::RepeatMode;
use super::window::{PLAYER_LABEL, is_music_page};
use crate::error::{AppError, ErrorKind};
use crate::session::AuthStatus;
use crate::state::AppState;

/// Longest queue `play_queue` accepts.
pub const MAX_QUEUE_LEN: usize = 500;

/// Reply to `player_snapshot`: the state the events would have sent so far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlayerSnapshot {
    pub state: PlayerState,
    pub queue: QueueState,
}

/// Replaces the queue with `songs` and plays `songs[start_index]`.
#[tauri::command]
#[specta::specta]
pub async fn play_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    songs: Vec<Song>,
    start_index: u32,
) -> Result<(), AppError> {
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    if songs.len() > MAX_QUEUE_LEN {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("A queue holds at most {MAX_QUEUE_LEN} songs."),
        ));
    }
    let start = usize::try_from(start_index).unwrap_or(usize::MAX);
    state
        .player
        .update(&app, |player, now| {
            player
                .play_songs(songs, start, now)
                .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))
        })
        .await
}

/// Pauses when playing or loading; otherwise plays.
#[tauri::command]
#[specta::specta]
pub async fn toggle_playback(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.toggle(now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn next_track(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.next(now)))
        .await
}

/// Restarts the song after three seconds; otherwise plays the previous song.
#[tauri::command]
#[specta::specta]
pub async fn previous_track(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.previous(now)))
        .await
}

/// Seeks to `position` seconds. Ignored while an ad plays.
#[tauri::command]
#[specta::specta]
pub async fn seek_to(
    app: AppHandle,
    state: State<'_, AppState>,
    position: f64,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.seek(position, now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_repeat(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: RepeatMode,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.set_repeat(mode, now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_shuffle(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.set_shuffle(enabled, now)))
        .await
}

/// The current player state and queue, for a UI that starts after playback.
#[tauri::command]
#[specta::specta]
pub async fn player_snapshot(state: State<'_, AppState>) -> Result<PlayerSnapshot, AppError> {
    let (state, queue) = state.player.snapshot().await;
    Ok(PlayerSnapshot { state, queue })
}

/// Called by the player window's init script. Rejects calls from any other
/// window or origin, even though the capability already limits callers.
#[tauri::command]
#[specta::specta]
pub async fn playback_event(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AppState>,
    event: PlaybackEvent,
) -> Result<(), AppError> {
    let from_player =
        webview.label() == PLAYER_LABEL && webview.url().is_ok_and(|url| is_music_page(&url));
    if !from_player {
        return Err(AppError::internal(
            "playback events come only from the player window",
        ));
    }
    state
        .player
        .update(&app, |player, now| Ok(player.handle_event(event, now)))
        .await
}
```

Replace `app/src-tauri/src/player/mod.rs` with:

```rust
//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

pub mod commands;
pub mod engine;
pub mod queue;
pub mod service;
pub mod video_id;
pub mod window;
```

Replace `app/src-tauri/src/events.rs` with:

```rust
use serde::{Deserialize, Serialize};

use crate::player::engine::{PlayerState, QueueState};
use crate::session::AuthStatus;

/// Emitted on `auth://changed` whenever the sign-in state changes.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event,
)]
#[tauri_specta(event_name = "auth://changed")]
pub struct AuthChanged {
    pub status: AuthStatus,
}

/// Emitted to the main window on `player://state`: on every status, track, or
/// ad change, and at most four times a second for progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "player://state")]
pub struct PlayerStateChanged {
    pub state: PlayerState,
}

/// Emitted to the main window on `player://queue` when the queue or the
/// current position in it changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "player://queue")]
pub struct QueueChanged {
    pub queue: QueueState,
}
```

In `app/src-tauri/src/error.rs`, add a variant to `ErrorKind` right after `NotSignedIn,`:

```rust
    /// A command argument was rejected, such as an unplayable queue position.
    InvalidArgument,
```

In `app/src-tauri/src/state.rs`, add `use crate::player::service::PlayerService;` after `use crate::error::AppError;`, add the field `pub player: PlayerService,` after `pub launch: LaunchGate,` in `AppState`, and `player: PlayerService::new(),` after `launch: LaunchGate::new(),` in `AppState::new`.

In `app/src-tauri/src/commands.rs`, replace the doc comment and signature of `sign_out` down to and including its `close_login_window` block so the function starts like this (the rest of the function is unchanged):

```rust
/// Stops playback, forgets the stored session, and deletes the WebView's
/// Google cookies.
#[tauri::command]
#[specta::specta]
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        close_login_window(&window);
    }
    // Stop playback and close the signed-in player page before the cookies go.
    if let Err(error) = state
        .player
        .update(&app, |player, now| Ok(player.stop(now)))
        .await
    {
        tracing::warn!(%error, "could not stop the player during sign-out");
    }
```

- [ ] **Step 5: Register commands, events, permissions, and capabilities**

Replace `app/src-tauri/src/lib.rs` with:

```rust
pub mod commands;
pub mod error;
pub mod events;
pub mod login;
pub mod player;
pub mod session;
pub mod state;
pub mod webview_session;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::session::Session;
use crate::session::store::{STORE_ENV, store_for, store_kind};
use crate::state::AppState;
use crate::webview_session::refresh_session_from_webview;

/// Commands and events exposed to the UI. `bindings_are_current` exports
/// them to `app/src/bindings.ts`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::auth_status,
            commands::sign_in,
            commands::sign_out,
            commands::get_home,
            player::commands::play_queue,
            player::commands::toggle_playback,
            player::commands::next_track,
            player::commands::previous_track,
            player::commands::seek_to,
            player::commands::set_repeat,
            player::commands::set_shuffle,
            player::commands::player_snapshot,
            player::commands::playback_event,
        ])
        .events(collect_events![
            events::AuthChanged,
            events::PlayerStateChanged,
            events::QueueChanged
        ])
        .disable_serde_phases()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kaset_lib=info,ytm_core=info".into()),
        )
        .init();

    let builder = specta_builder();
    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let data_dir = app.path().app_data_dir()?;
            let kind = store_kind(
                std::env::var(STORE_ENV).ok().as_deref(),
                cfg!(debug_assertions),
            );
            tracing::info!(store = ?kind, "session store selected");
            let session = Session::new(store_for(kind, &data_dir));
            let status = session.restore();
            tracing::info!(?status, "session restored");
            app.manage(AppState::new(session));
            refresh_cookies_from_webview(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The hidden player window would keep the app (and audio) alive
            // after the user closes the main window.
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Kaset");
}

/// Picks up cookies the WebView rotated since the session was saved, then
/// opens the launch gate that API commands wait on.
/// `Session::refresh` ignores this unless the user is signed in.
fn refresh_cookies_from_webview(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        refresh_session_from_webview(&app, "main", "launch").await;
        app.state::<AppState>().launch.open();
    });
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;

    /// Regenerates `app/src/bindings.ts`. CI fails if the committed file differs.
    #[test]
    fn bindings_are_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");
        super::specta_builder()
            .mocks()
            .export(Typescript::default().header("// @ts-nocheck"), path)
            .expect("failed to export bindings");
    }
}
```

Replace `app/src-tauri/build.rs` with:

```rust
fn main() {
    // Test executables need the manifest too, but tauri-build links it only into binaries.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
        let manifest = std::path::Path::new(&manifest_dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&[
                "auth_status",
                "sign_in",
                "sign_out",
                "get_home",
                "play_queue",
                "toggle_playback",
                "next_track",
                "previous_track",
                "seek_to",
                "set_repeat",
                "set_shuffle",
                "player_snapshot",
                "playback_event",
            ]))
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");
}
```

Replace `app/src-tauri/capabilities/default.json` with:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Local UI in the main window. The login window has no capability, so its remote pages cannot call any command.",
  "windows": ["main"],
  "permissions": [
    "core:event:default",
    "allow-auth-status",
    "allow-sign-in",
    "allow-sign-out",
    "allow-get-home",
    "allow-play-queue",
    "allow-toggle-playback",
    "allow-next-track",
    "allow-previous-track",
    "allow-seek-to",
    "allow-set-repeat",
    "allow-set-shuffle",
    "allow-player-snapshot"
  ]
}
```

Create `app/src-tauri/capabilities/player.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "player-remote",
  "description": "The hidden player window's https://music.youtube.com pages may report playback events and nothing else: no core or event permissions.",
  "windows": ["player"],
  "remote": { "urls": ["https://music.youtube.com/*"] },
  "permissions": ["allow-playback-event"]
}
```

- [ ] **Step 6: Build, regenerate, and check**

Run: `cargo fmt --all && cargo test -p kaset`
Expected: `test result: ok. 98 passed`. The build generates the nine `permissions/autogenerated/*.toml` files listed above, and `bindings_are_current` rewrites `app/src/bindings.ts`.

Check the generated bindings: `grep -nE "playQueue: \(songs: Song\[\], startIndex: number|playerState: makeEvent|playerQueue: makeEvent|export type PlaybackEvent|\"invalid_argument\"" app/src/bindings.ts`
Expected: five matching lines.

Check the capability: `grep -c '"allow-playback-event"' app/src-tauri/capabilities/default.json` prints `0`, and `app/src-tauri/capabilities/player.json` lists only `allow-playback-event`.

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo fmt --all --check`
Expected: all pass.

Run (from `app/`): `pnpm typecheck && pnpm lint && pnpm test`
Expected: all pass (the UI does not use the new bindings yet).

- [ ] **Step 7: Commit and push**

```bash
git add app/src-tauri/src/player/ app/src-tauri/src/events.rs app/src-tauri/src/error.rs app/src-tauri/src/state.rs app/src-tauri/src/commands.rs app/src-tauri/src/session/cookies.rs app/src-tauri/src/lib.rs app/src-tauri/build.rs app/src-tauri/capabilities/default.json app/src-tauri/capabilities/player.json app/src-tauri/permissions/autogenerated/ app/src/bindings.ts
git commit -m "feat(app): play songs in a hidden player window"
git push origin main
```

---

### Task 5: OS media controls with souvlaki

Publishes the player state to macOS Now Playing / Windows SMTC and routes media keys and OS controls back into the engine.

souvlaki facts this task relies on (from `souvlaki-0.9.3/src`): `MediaControls::new(PlatformConfig { display_name, dbus_name, hwnd: Option<*mut c_void> })` — on Windows `hwnd` must be `Some` (it `expect`s); `attach<F: Fn(MediaControlEvent) + Send + 'static>`; `set_metadata(MediaMetadata { title, album, artist, cover_url, duration })`; `set_playback(MediaPlayback::{Stopped, Paused { progress }, Playing { progress }})`; `MediaControlEvent` is `#[non_exhaustive]`. On macOS the handle is a unit struct calling `MPNowPlayingInfoCenter` / `MPRemoteCommandCenter`, whose handlers run on the main thread and need the app's event loop (Tauri's); on Windows it wraps SMTC for the given window. Hence the main-thread `thread_local`.

**Files:**
- Modify: `app/src-tauri/Cargo.toml`, `Cargo.lock`, `app/src-tauri/src/player/mod.rs`, `app/src-tauri/src/player/service.rs`, `app/src-tauri/src/lib.rs`
- Create: `app/src-tauri/src/player/media.rs`
- Test: `app/src-tauri/src/player/media.rs`

**Interfaces:**
- Consumes: Task 3 `Player`, `PlayerState`, `PlaybackStatus`, `Outcome`; Task 4 `PlayerService::update`, `AppState`.
- Produces: `player::media::{SEEK_STEP_SECONDS = 10.0, NowPlaying, Playback::{Stopped, Paused(f64), Playing(f64)}, MediaAction::{Play, Pause, Toggle, Next, Previous, SeekTo(f64), SeekBy(f64)}, MediaAction::from_event(&MediaControlEvent) -> Option<MediaAction>, MediaAction::apply(self, &mut Player, Instant) -> Outcome, now_playing(&PlayerState) -> Option<NowPlaying>, playback(&PlayerState) -> Playback, MediaUpdate { metadata, playback }, MediaPublisher::update(&mut self, &PlayerState, Instant) -> MediaUpdate, attach(&AppHandle), publish(&AppHandle, MediaUpdate)}`. `PlayerService::update` now also publishes media updates.

- [ ] **Step 1: Add the dependency and the failing tests**

In `app/src-tauri/Cargo.toml`, add under `[dependencies]` after `serde_json = "1"`:

```toml
souvlaki = "0.9.3"
```

Create `app/src-tauri/src/player/media.rs` containing only this tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::queue::RepeatMode;
    use ytm_core::models::{ArtistRef, Song};

    fn state(status: PlaybackStatus, position: f64) -> PlayerState {
        PlayerState {
            status,
            track: Some(Song {
                video_id: "test-vid-0A".into(),
                title: "Test song".into(),
                artists: vec![
                    ArtistRef {
                        name: "Artist one".into(),
                        browse_id: None,
                    },
                    ArtistRef {
                        name: "Artist two".into(),
                        browse_id: None,
                    },
                ],
                thumbnail_url: Some("https://lh3.googleusercontent.com/test".into()),
            }),
            position,
            duration: 200.0,
            is_ad: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            has_previous: false,
            has_next: false,
        }
    }

    #[test]
    fn maps_media_events() {
        use MediaControlEvent as E;
        assert_eq!(
            MediaAction::from_event(&E::Toggle),
            Some(MediaAction::Toggle)
        );
        assert_eq!(MediaAction::from_event(&E::Stop), Some(MediaAction::Pause));
        assert_eq!(
            MediaAction::from_event(&E::SetPosition(MediaPosition(Duration::from_secs(42)))),
            Some(MediaAction::SeekTo(42.0))
        );
        assert_eq!(
            MediaAction::from_event(&E::SeekBy(SeekDirection::Backward, Duration::from_secs(5))),
            Some(MediaAction::SeekBy(-5.0))
        );
        assert_eq!(
            MediaAction::from_event(&E::Seek(SeekDirection::Forward)),
            Some(MediaAction::SeekBy(SEEK_STEP_SECONDS))
        );
        assert_eq!(MediaAction::from_event(&E::Raise), None);
        assert_eq!(
            MediaAction::from_event(&E::OpenUri("https://example.com".into())),
            None
        );
    }

    #[test]
    fn now_playing_joins_artists_and_hides_ad_duration() {
        let info = now_playing(&state(PlaybackStatus::Playing, 1.0)).unwrap();
        assert_eq!(info.title, "Test song");
        assert_eq!(info.artist, "Artist one, Artist two");
        assert_eq!(info.duration, Some(Duration::from_secs(200)));
        let mut ad = state(PlaybackStatus::Playing, 1.0);
        ad.is_ad = true;
        assert_eq!(now_playing(&ad).unwrap().duration, None);
        assert_eq!(playback(&ad), Playback::Playing(0.0));
    }

    #[test]
    fn publisher_sends_metadata_once_and_skips_steady_progress() {
        let mut publisher = MediaPublisher::default();
        let start = Instant::now();
        let first = publisher.update(&state(PlaybackStatus::Playing, 10.0), start);
        assert!(first.metadata.is_some());
        assert_eq!(first.playback, Some(Playback::Playing(10.0)));

        let later = start + Duration::from_secs(2);
        assert_eq!(
            publisher.update(&state(PlaybackStatus::Playing, 12.0), later),
            MediaUpdate::default()
        );
        // A seek moves the position away from the extrapolated one.
        let seeked = publisher.update(&state(PlaybackStatus::Playing, 90.0), later);
        assert_eq!(seeked.playback, Some(Playback::Playing(90.0)));
        assert!(seeked.metadata.is_none());
        // Pausing is a change.
        let paused = publisher.update(&state(PlaybackStatus::Paused, 90.0), later);
        assert_eq!(paused.playback, Some(Playback::Paused(90.0)));
    }

    #[test]
    fn publisher_reports_stop_when_the_queue_empties() {
        let mut publisher = MediaPublisher::default();
        let now = Instant::now();
        publisher.update(&state(PlaybackStatus::Playing, 1.0), now);
        let mut idle = state(PlaybackStatus::Idle, 0.0);
        idle.track = None;
        let update = publisher.update(&idle, now);
        assert_eq!(update.playback, Some(Playback::Stopped));
    }

    #[test]
    fn seek_by_is_relative_to_the_current_position() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player
            .play_songs(
                vec![Song {
                    video_id: "test-vid-0A".into(),
                    title: "Test song".into(),
                    artists: Vec::new(),
                    thumbnail_url: None,
                }],
                0,
                now,
            )
            .unwrap();
        player.handle_event(
            crate::player::engine::PlaybackEvent::Progress {
                video_id: crate::player::video_id::VideoId::parse("test-vid-0A").unwrap(),
                playing: true,
                position: 30.0,
                duration: 200.0,
                ad: false,
            },
            now,
        );
        let outcome = MediaAction::SeekBy(-10.0).apply(&mut player, now);
        assert_eq!(
            outcome.command,
            Some(crate::player::engine::WebCommand::Seek(20.0))
        );
    }
}
```

Replace `app/src-tauri/src/player/mod.rs` with:

```rust
//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

pub mod commands;
pub mod engine;
pub mod media;
pub mod queue;
pub mod service;
pub mod video_id;
pub mod window;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset player::media`
Expected: compile errors such as `cannot find type 'MediaAction' in this scope`, `cannot find type 'MediaPublisher'`.

- [ ] **Step 3: Implement the media module**

Insert above the tests module in `app/src-tauri/src/player/media.rs`:

```rust
//! OS media controls through souvlaki: macOS Now Playing and media keys,
//! Windows System Media Transport Controls (SMTC) and media keys.
//!
//! souvlaki's handle is created, updated, and dropped on the main thread only:
//! on macOS it registers `MPRemoteCommandCenter` handlers, and on Windows it
//! binds SMTC to the main window's HWND. The handle lives in a main-thread
//! thread-local and every call goes through `AppHandle::run_on_main_thread`.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Manager};

use super::engine::{Outcome, PlaybackStatus, Player, PlayerState};
use crate::state::AppState;

/// Seek step for media keys that do not say how far (Windows fast-forward/rewind).
pub const SEEK_STEP_SECONDS: f64 = 10.0;

thread_local! {
    static CONTROLS: RefCell<Option<MediaControls>> = const { RefCell::new(None) };
}

/// Track metadata the OS shows.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
}

/// Playback state the OS shows, with the position in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Playback {
    Stopped,
    Paused(f64),
    Playing(f64),
}

/// A media key or OS control, mapped to a player change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaAction {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    SeekTo(f64),
    SeekBy(f64),
}

impl MediaAction {
    pub fn from_event(event: &MediaControlEvent) -> Option<Self> {
        let step = |direction: &SeekDirection, seconds: f64| match direction {
            SeekDirection::Forward => seconds,
            SeekDirection::Backward => -seconds,
        };
        Some(match event {
            MediaControlEvent::Play => Self::Play,
            MediaControlEvent::Pause | MediaControlEvent::Stop => Self::Pause,
            MediaControlEvent::Toggle => Self::Toggle,
            MediaControlEvent::Next => Self::Next,
            MediaControlEvent::Previous => Self::Previous,
            MediaControlEvent::SetPosition(MediaPosition(position)) => {
                Self::SeekTo(position.as_secs_f64())
            }
            MediaControlEvent::SeekBy(direction, amount) => {
                Self::SeekBy(step(direction, amount.as_secs_f64()))
            }
            MediaControlEvent::Seek(direction) => Self::SeekBy(step(direction, SEEK_STEP_SECONDS)),
            _ => return None,
        })
    }

    pub fn apply(self, player: &mut Player, now: Instant) -> Outcome {
        match self {
            Self::Play => player.play(now),
            Self::Pause => player.pause(now),
            Self::Toggle => player.toggle(now),
            Self::Next => player.next(now),
            Self::Previous => player.previous(now),
            Self::SeekTo(position) => player.seek(position, now),
            Self::SeekBy(delta) => {
                let position = player.state().position;
                player.seek(position + delta, now)
            }
        }
    }
}

pub fn now_playing(state: &PlayerState) -> Option<NowPlaying> {
    let track = state.track.as_ref()?;
    let artist = track
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(NowPlaying {
        title: track.title.clone(),
        artist,
        cover_url: track.thumbnail_url.clone(),
        duration: (state.duration > 0.0 && !state.is_ad)
            .then(|| Duration::from_secs_f64(state.duration)),
    })
}

pub fn playback(state: &PlayerState) -> Playback {
    let position = if state.is_ad { 0.0 } else { state.position };
    match state.status {
        PlaybackStatus::Idle => Playback::Stopped,
        PlaybackStatus::Playing => Playback::Playing(position),
        PlaybackStatus::Loading | PlaybackStatus::Paused | PlaybackStatus::Ended => {
            Playback::Paused(position)
        }
    }
}

/// What to send to the OS for one player state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaUpdate {
    pub metadata: Option<NowPlaying>,
    pub playback: Option<Playback>,
}

/// Remembers what the OS was last told, so steady progress does not resend
/// anything: the OS advances a playing position on its own.
#[derive(Debug, Default)]
pub struct MediaPublisher {
    metadata: Option<NowPlaying>,
    playback: Option<(Playback, Instant)>,
}

/// A playing position further than this from the extrapolated one is a seek.
const DRIFT_SECONDS: f64 = 1.5;

impl MediaPublisher {
    pub fn update(&mut self, state: &PlayerState, now: Instant) -> MediaUpdate {
        let mut update = MediaUpdate::default();
        let metadata = now_playing(state);
        if metadata != self.metadata {
            update.metadata.clone_from(&metadata);
            self.metadata = metadata;
            // New metadata resets the position the OS shows.
            self.playback = None;
        }
        let next = playback(state);
        let changed = match (self.playback, next) {
            (None, _) => true,
            (Some((Playback::Playing(was), at)), Playback::Playing(is)) => {
                let expected = was + now.saturating_duration_since(at).as_secs_f64();
                (is - expected).abs() > DRIFT_SECONDS
            }
            (Some((Playback::Paused(was), _)), Playback::Paused(is)) => (is - was).abs() > 0.5,
            (Some((previous, _)), next) => {
                std::mem::discriminant(&previous) != std::mem::discriminant(&next)
            }
        };
        if changed {
            update.playback = Some(next);
            self.playback = Some((next, now));
        }
        update
    }
}

/// Creates the OS media controls on the main thread and routes their events
/// to the player. Failure is logged; the app works without media keys.
pub fn attach(app: &AppHandle) {
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        #[cfg(windows)]
        let hwnd = match handle
            .get_webview_window("main")
            .map(|window| window.hwnd())
        {
            Some(Ok(hwnd)) => Some(hwnd.0),
            _ => {
                tracing::warn!("no main window handle; media controls disabled");
                return;
            }
        };
        #[cfg(not(windows))]
        let hwnd = None;
        let config = PlatformConfig {
            display_name: "Kaset",
            dbus_name: "kaset",
            hwnd,
        };
        let mut controls = match MediaControls::new(config) {
            Ok(controls) => controls,
            Err(error) => {
                tracing::warn!(?error, "could not create media controls");
                return;
            }
        };
        let events_app = handle.clone();
        let attached = controls.attach(move |event| {
            let Some(action) = MediaAction::from_event(&event) else {
                return;
            };
            let app = events_app.clone();
            // Handlers run on the main thread on macOS; never lock the player there.
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                if let Err(error) = state
                    .player
                    .update(&app, |player, now| Ok(action.apply(player, now)))
                    .await
                {
                    tracing::warn!(%error, "media control action failed");
                }
            });
        });
        if let Err(error) = attached {
            tracing::warn!(?error, "could not attach media controls");
            return;
        }
        CONTROLS.with(|slot| *slot.borrow_mut() = Some(controls));
    });
    if let Err(error) = result {
        tracing::warn!(%error, "could not schedule media controls setup");
    }
}

/// Sends `update` to the OS on the main thread.
pub fn publish(app: &AppHandle, update: MediaUpdate) {
    if update == MediaUpdate::default() {
        return;
    }
    let result = app.run_on_main_thread(move || {
        CONTROLS.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(controls) = slot.as_mut() else {
                return;
            };
            if let Some(metadata) = &update.metadata {
                let result = controls.set_metadata(MediaMetadata {
                    title: Some(&metadata.title),
                    artist: Some(&metadata.artist),
                    album: None,
                    cover_url: metadata.cover_url.as_deref(),
                    duration: metadata.duration,
                });
                if let Err(error) = result {
                    tracing::warn!(?error, "could not update media metadata");
                }
            }
            if let Some(playback) = update.playback {
                let position =
                    |seconds: f64| Some(MediaPosition(Duration::from_secs_f64(seconds.max(0.0))));
                let playback = match playback {
                    Playback::Stopped => MediaPlayback::Stopped,
                    Playback::Paused(seconds) => MediaPlayback::Paused {
                        progress: position(seconds),
                    },
                    Playback::Playing(seconds) => MediaPlayback::Playing {
                        progress: position(seconds),
                    },
                };
                if let Err(error) = controls.set_playback(playback) {
                    tracing::warn!(?error, "could not update media playback state");
                }
            }
        });
    });
    if let Err(error) = result {
        tracing::warn!(%error, "could not schedule a media controls update");
    }
}
```

- [ ] **Step 4: Publish from the service and attach at launch**

Replace `app/src-tauri/src/player/service.rs` with:

```rust
//! Applies engine outcomes: drives the player window, emits `player://state`
//! and `player://queue` to the main window, and updates the OS media controls.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::Mutex;

use super::engine::{Outcome, Player, PlayerState, QueueState, WebCommand};
use super::media::{self, MediaPublisher};
use super::window;
use crate::error::AppError;
use crate::events::{PlayerStateChanged, QueueChanged};

/// Player events go to the main window. The player window's capability has
/// no event permissions, so its pages cannot listen to them either.
const UI_WINDOW: &str = "main";

struct Inner {
    player: Player,
    media: MediaPublisher,
}

pub struct PlayerService {
    // A tokio mutex, held while an outcome is applied, so window commands run
    // in the order the engine produced them. Never locked on the main thread.
    inner: Mutex<Inner>,
}

impl PlayerService {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1);
        Self {
            inner: Mutex::new(Inner {
                player: Player::new(seed),
                media: MediaPublisher::default(),
            }),
        }
    }

    /// Runs `change` on the engine and applies its outcome.
    pub async fn update<F>(&self, app: &AppHandle, change: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>,
    {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let outcome = change(&mut inner.player, now)?;
        if let Some(state) = &outcome.state {
            let update = inner.media.update(state, now);
            media::publish(app, update);
        }
        apply(app, outcome)
    }

    pub async fn snapshot(&self) -> (PlayerState, QueueState) {
        let inner = self.inner.lock().await;
        (inner.player.state(), inner.player.queue_state())
    }
}

impl Default for PlayerService {
    fn default() -> Self {
        Self::new()
    }
}

fn apply(app: &AppHandle, outcome: Outcome) -> Result<(), AppError> {
    let result = match outcome.command {
        Some(WebCommand::Load(id)) => window::load(app, &id),
        Some(WebCommand::Play) => window::run_script(app, window::play_script()),
        Some(WebCommand::Pause) => window::run_script(app, window::pause_script()),
        Some(WebCommand::Seek(position)) => window::run_script(app, &window::seek_script(position)),
        Some(WebCommand::Stop) => {
            window::close(app);
            Ok(())
        }
        None => Ok(()),
    };
    if let Some(queue) = outcome.queue
        && let Err(error) = (QueueChanged { queue }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://queue");
    }
    if let Some(state) = outcome.state
        && let Err(error) = (PlayerStateChanged { state }).emit_to(app, UI_WINDOW)
    {
        tracing::warn!(%error, "could not emit player://state");
    }
    result
}
```

In `app/src-tauri/src/lib.rs`, inside `setup`, add `player::media::attach(app.handle());` on the line after `app.manage(AppState::new(session));`.

- [ ] **Step 5: Run the tests and checks**

Run: `cargo fmt --all && cargo test -p kaset`
Expected: `test result: ok. 103 passed`. `app/src/bindings.ts` is unchanged.

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo fmt --all --check`
Expected: all pass. A `future-incompat` note about `block v0.1.6` (a souvlaki dependency on macOS) is a warning from Cargo, not a failure.

- [ ] **Step 6: Commit, push, and verify Windows CI**

```bash
git add app/src-tauri/Cargo.toml Cargo.lock app/src-tauri/src/player/media.rs app/src-tauri/src/player/mod.rs app/src-tauri/src/player/service.rs app/src-tauri/src/lib.rs
git commit -m "feat(app): publish playback to OS media controls"
git push origin main
gh run list --limit 4
```

The `#[cfg(windows)]` branch of `attach` is compiled only on Windows. Wait for the `Rust` and `App` runs on this commit: `gh run watch <run-id> --exit-status` for each. Expected: both succeed on `macos-latest` and `windows-latest`. If Windows fails to compile `media.rs`, read `gh run view <run-id> --log-failed`, fix the Windows branch only, and push a follow-up commit `fix(app): <what> in Windows media controls`.

---

### Task 6: React player bar and playable home songs

**Files:**
- Create: `app/src/player/format.ts`, `app/src/player/usePlayer.ts`, `app/src/player/PlayerBar.tsx`, `app/src/player/testing.ts`, `app/src/player/PlayerBar.test.tsx`
- Modify: `app/src/home/HomeScreen.tsx`, `app/src/App.tsx`, `app/src/App.test.tsx`, `app/src/styles.css`

**Interfaces:**
- Consumes (from the generated `app/src/bindings.ts`): `commands.{playQueue(songs, startIndex), togglePlayback(), nextTrack(), previousTrack(), seekTo(position), setRepeat(mode), setShuffle(enabled), playerSnapshot()}`, `events.playerState` (payload `{ state: PlayerState }`), `events.playerQueue` (payload `{ queue: QueueState }`), types `PlayerState`, `QueueState`, `PlayerSnapshot`, `RepeatMode`, `Song`, `ShelfItem`. `PlayerState.position`/`duration` are `number | null`. `mockCommands` handlers receive the invoke args object (for example `{ mode: "all" }`).
- Produces: `formatTime(seconds: number | null): string`; `usePlayer(): { state: PlayerState | null; queue: QueueState | null }`; `<PlayerBar />` (a `<footer aria-label="Player">`, rendered only when a track is set); test fixtures `testSong`, `idleSnapshot`, `playingState(overrides)` in `player/testing.ts`.

- [ ] **Step 1: Write the failing tests**

Create `app/src/player/testing.ts`:

```typescript
import type { PlayerSnapshot, PlayerState, Song } from "../bindings";

export const testSong: Song = {
  video_id: "test-vid-0A",
  title: "Test song",
  artists: [{ name: "Test artist", browse_id: null }],
  thumbnail_url: null,
};

export const idleSnapshot: PlayerSnapshot = {
  state: {
    status: "idle",
    track: null,
    position: 0,
    duration: 0,
    is_ad: false,
    repeat: "off",
    shuffle: false,
    has_previous: false,
    has_next: false,
  },
  queue: { songs: [], current: null },
};

export function playingState(overrides: Partial<PlayerState> = {}): PlayerState {
  return {
    status: "playing",
    track: testSong,
    position: 65,
    duration: 200,
    is_ad: false,
    repeat: "off",
    shuffle: false,
    has_previous: false,
    has_next: true,
    ...overrides,
  };
}
```

Create `app/src/player/PlayerBar.test.tsx`:

```tsx
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { events, mockCommands } from "../bindings";
import { formatTime } from "./format";
import { PlayerBar } from "./PlayerBar";
import { idleSnapshot, playingState, testSong } from "./testing";

function renderPlaying(overrides: Parameters<typeof playingState>[0] = {}) {
  const handlers = {
    playerSnapshot: () => ({
      state: playingState(overrides),
      queue: { songs: [testSong, testSong], current: 0 },
    }),
    togglePlayback: vi.fn(() => null),
    nextTrack: vi.fn(() => null),
    previousTrack: vi.fn(() => null),
    seekTo: vi.fn(() => null),
    setRepeat: vi.fn(() => null),
    setShuffle: vi.fn(() => null),
  };
  mockCommands(handlers);
  render(<PlayerBar />);
  return handlers;
}

test("formats times as m:ss", () => {
  expect(formatTime(0)).toBe("0:00");
  expect(formatTime(65.9)).toBe("1:05");
  expect(formatTime(3600)).toBe("60:00");
  expect(formatTime(null)).toBe("0:00");
  expect(formatTime(-3)).toBe("0:00");
});

test("stays hidden while nothing is queued", async () => {
  const playerSnapshot = vi.fn(() => idleSnapshot);
  mockCommands({ playerSnapshot });
  const { container } = render(<PlayerBar />);
  await vi.waitFor(() => expect(playerSnapshot).toHaveBeenCalled());
  expect(container).toBeEmptyDOMElement();
});

test("shows the track from player://state", async () => {
  mockCommands({ playerSnapshot: () => idleSnapshot });
  render(<PlayerBar />);
  await act(() => events.playerState.emit({ state: playingState() }));
  expect(await screen.findByText("Test song")).toBeInTheDocument();
  expect(screen.getByText("Test artist")).toBeInTheDocument();
  expect(screen.getByText("1:05")).toBeInTheDocument();
  expect(screen.getByText("3:20")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
});

test("follows player://queue for the position in the queue", async () => {
  renderPlaying();
  expect(await screen.findByText("1 of 2")).toBeInTheDocument();
  await act(() =>
    events.playerQueue.emit({ queue: { songs: [testSong, testSong, testSong], current: 2 } }),
  );
  expect(await screen.findByText("3 of 3")).toBeInTheDocument();
});

test("transport buttons call the player commands", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Pause" }));
  await userEvent.click(screen.getByRole("button", { name: "Next" }));
  await userEvent.click(screen.getByRole("button", { name: "Previous" }));
  expect(handlers.togglePlayback).toHaveBeenCalledTimes(1);
  expect(handlers.nextTrack).toHaveBeenCalledTimes(1);
  expect(handlers.previousTrack).toHaveBeenCalledTimes(1);
});

test("next is disabled at the end of the queue", async () => {
  renderPlaying({ has_next: false, status: "paused" });
  expect(await screen.findByRole("button", { name: "Next" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
});

test("repeat cycles off, all, one and shuffle toggles", async () => {
  const handlers = renderPlaying();
  await userEvent.click(await screen.findByRole("button", { name: "Repeat off" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "all" });
  await act(() => events.playerState.emit({ state: playingState({ repeat: "one", shuffle: true }) }));
  await userEvent.click(await screen.findByRole("button", { name: "Repeat one" }));
  expect(handlers.setRepeat).toHaveBeenLastCalledWith({ mode: "off" });
  const shuffle = screen.getByRole("button", { name: "Shuffle" });
  expect(shuffle).toHaveAttribute("aria-pressed", "true");
  await userEvent.click(shuffle);
  expect(handlers.setShuffle).toHaveBeenLastCalledWith({ enabled: false });
});

test("seeking commits when the slider is released", async () => {
  const handlers = renderPlaying();
  const slider = await screen.findByRole("slider", { name: "Seek" });
  fireEvent.change(slider, { target: { value: "120" } });
  expect(handlers.seekTo).not.toHaveBeenCalled();
  expect(screen.getByText("2:00")).toBeInTheDocument();
  fireEvent.pointerUp(slider);
  expect(handlers.seekTo).toHaveBeenCalledWith({ position: 120 });
});

test("an ad replaces the timeline", async () => {
  renderPlaying({ is_ad: true });
  expect(await screen.findByText("Ad")).toBeInTheDocument();
  expect(screen.queryByRole("slider", { name: "Seek" })).not.toBeInTheDocument();
});

test("command errors are shown", async () => {
  mockCommands({
    playerSnapshot: () => ({ state: playingState(), queue: idleSnapshot.queue }),
    nextTrack: () => {
      throw { kind: "internal", message: "player window failed" };
    },
  });
  render(<PlayerBar />);
  await userEvent.click(await screen.findByRole("button", { name: "Next" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("player window failed");
});
```

Replace `app/src/App.test.tsx` with:

```tsx
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { App } from "./App";
import { events, mockCommands, type HomePage } from "./bindings";
import { idleSnapshot, playingState } from "./player/testing";

const home: HomePage = {
  sections: [
    { title: "Listen again", items: [] },
    {
      title: "Quick picks",
      items: [
        {
          kind: "song",
          video_id: "test-vid-01",
          title: "First song",
          artists: [{ name: "Test artist", browse_id: null }],
          thumbnail_url: null,
        },
        {
          kind: "album",
          browse_id: "test-album",
          title: "Test album",
          subtitle: null,
          thumbnail_url: null,
        },
        {
          kind: "song",
          video_id: "test-vid-02",
          title: "Second song",
          artists: [],
          thumbnail_url: null,
        },
      ],
    },
  ],
  continuation: null,
};

test("signed out shows the sign-in screen", async () => {
  mockCommands({ authStatus: () => "signed_out" });
  render(<App />);
  expect(await screen.findByRole("button", { name: "Sign in with Google" })).toBeInTheDocument();
});

test("signed in lists home section titles", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
  expect(screen.getByText("Quick picks (3)")).toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({
    authStatus: () => "signed_out",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByText("Quick picks (3)")).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("home errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
    playerSnapshot: () => idleSnapshot,
    getHome: () => {
      calls += 1;
      if (calls === 1) throw { kind: "network", message: "network error" };
      return home;
    },
  });
  render(<App />);

  expect(await screen.findByRole("alert")).toHaveTextContent("network error");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
});

test("sign out calls the command", async () => {
  const signOut = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    signOut,
  });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});

test("clicking a song plays its shelf's songs from that song", async () => {
  const playQueue = vi.fn(() => null);
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    playQueue,
  });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Second song" }));
  expect(playQueue).toHaveBeenCalledWith({
    songs: [
      {
        video_id: "test-vid-01",
        title: "First song",
        artists: [{ name: "Test artist", browse_id: null }],
        thumbnail_url: null,
      },
      { video_id: "test-vid-02", title: "Second song", artists: [], thumbnail_url: null },
    ],
    startIndex: 1,
  });
  expect(screen.getByRole("button", { name: "First song · Test artist" })).toBeInTheDocument();
});

test("the player bar appears when playback starts", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
  });
  render(<App />);
  await screen.findByText("Listen again (0)");
  expect(screen.queryByRole("contentinfo", { name: "Player" })).not.toBeInTheDocument();

  await act(() => events.playerState.emit({ state: playingState() }));
  expect(await screen.findByRole("contentinfo", { name: "Player" })).toHaveTextContent("Test song");
});

test("a failed play shows the error", async () => {
  mockCommands({
    authStatus: () => "signed_in",
    getHome: () => home,
    playerSnapshot: () => idleSnapshot,
    playQueue: () => {
      throw { kind: "not_signed_in", message: "Sign in to continue." };
    },
  });
  render(<App />);
  await userEvent.click(await screen.findByRole("button", { name: "Second song" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Sign in to continue.");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `app/`): `pnpm test`
Expected: FAIL — `Failed to resolve import "./PlayerBar"` / `"./format"`, and App tests cannot find the `Second song` button.

- [ ] **Step 3: Implement the player bar**

Create `app/src/player/format.ts`:

```typescript
/** `m:ss` for a position in seconds. Rust sends `null` for a non-finite number. */
export function formatTime(seconds: number | null): string {
  const total = seconds !== null && Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const minutes = Math.floor(total / 60);
  const rest = total % 60;
  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}
```

Create `app/src/player/usePlayer.ts`:

```typescript
import { useEffect, useState } from "react";
import { commands, events, type PlayerState, type QueueState } from "../bindings";

export type PlayerView = {
  state: PlayerState | null;
  queue: QueueState | null;
};

/**
 * The player state and queue: `null` until known, then kept current by
 * `player://state` and `player://queue`. Events that arrive before the
 * initial `playerSnapshot` reply win.
 */
export function usePlayer(): PlayerView {
  const [state, setState] = useState<PlayerState | null>(null);
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let active = true;
    const stopState = events.playerState.listen((event) => {
      if (active) setState(event.payload.state);
    });
    const stopQueue = events.playerQueue.listen((event) => {
      if (active) setQueue(event.payload.queue);
    });
    commands.playerSnapshot().then(
      (snapshot) => {
        if (!active) return;
        setState((previous) => previous ?? snapshot.state);
        setQueue((previous) => previous ?? snapshot.queue);
      },
      () => {
        // No snapshot: the bar stays hidden until the first event.
      },
    );
    return () => {
      active = false;
      void stopState.then((stop) => stop());
      void stopQueue.then((stop) => stop());
    };
  }, []);

  return { state, queue };
}
```

Create `app/src/player/PlayerBar.tsx`:

```tsx
import { useState } from "react";
import { commands, type RepeatMode } from "../bindings";
import { errorMessage } from "../errors";
import { formatTime } from "./format";
import { usePlayer } from "./usePlayer";

const NEXT_REPEAT: Record<RepeatMode, RepeatMode> = { off: "all", all: "one", one: "off" };
const REPEAT_LABEL: Record<RepeatMode, string> = {
  off: "Repeat off",
  all: "Repeat all",
  one: "Repeat one",
};

/** Now playing, transport controls, and progress. Hidden while nothing is queued. */
export function PlayerBar() {
  const { state, queue } = usePlayer();
  const [error, setError] = useState<string | null>(null);
  // While the user drags the seek slider, show the drag position.
  const [dragPosition, setDragPosition] = useState<number | null>(null);

  if (state === null || state.track === null) return null;

  function run(action: () => Promise<unknown>) {
    setError(null);
    action().catch((cause: unknown) => setError(errorMessage(cause)));
  }

  function commitSeek() {
    if (dragPosition === null) return;
    const position = dragPosition;
    setDragPosition(null);
    run(() => commands.seekTo(position));
  }

  const track = state.track;
  const artists = track.artists.map((artist) => artist.name).join(", ");
  const playing = state.status === "playing" || state.status === "loading";
  const duration = state.duration ?? 0;
  const position = dragPosition ?? state.position ?? 0;

  return (
    <footer className="player-bar" aria-label="Player">
      <div className="player-track">
        <strong>{track.title}</strong>
        {artists && <span>{artists}</span>}
        {queue && queue.current !== null && (
          <span className="player-queue">
            {queue.current + 1} of {queue.songs.length}
          </span>
        )}
      </div>
      <div className="player-controls">
        <button
          type="button"
          aria-pressed={state.shuffle}
          onClick={() => run(() => commands.setShuffle(!state.shuffle))}
        >
          Shuffle
        </button>
        <button type="button" onClick={() => run(() => commands.previousTrack())}>
          Previous
        </button>
        <button type="button" onClick={() => run(() => commands.togglePlayback())}>
          {playing ? "Pause" : "Play"}
        </button>
        <button
          type="button"
          disabled={!state.has_next}
          onClick={() => run(() => commands.nextTrack())}
        >
          Next
        </button>
        <button type="button" onClick={() => run(() => commands.setRepeat(NEXT_REPEAT[state.repeat]))}>
          {REPEAT_LABEL[state.repeat]}
        </button>
      </div>
      <div className="player-progress">
        {state.is_ad ? (
          <span className="player-ad">Ad</span>
        ) : (
          <>
            <span>{formatTime(position)}</span>
            <input
              type="range"
              aria-label="Seek"
              min={0}
              max={duration}
              step={1}
              value={Math.min(position, duration)}
              disabled={duration <= 0}
              onChange={(event) => setDragPosition(Number(event.target.value))}
              onPointerUp={commitSeek}
              onKeyUp={commitSeek}
            />
            <span>{formatTime(duration)}</span>
          </>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
    </footer>
  );
}
```

- [ ] **Step 4: Make home songs playable and mount the bar**

Replace `app/src/home/HomeScreen.tsx` with:

```tsx
import { useEffect, useState } from "react";
import { commands, type HomePage, type ShelfItem, type Song } from "../bindings";
import { errorMessage } from "../errors";

type State =
  | { kind: "loading" }
  | { kind: "loaded"; page: HomePage }
  | { kind: "error"; message: string };

function fetchHome(): Promise<State> {
  return commands.getHome().then(
    (page): State => ({ kind: "loaded", page }),
    // An auth_expired error also arrives as auth://changed, which replaces this screen.
    (cause: unknown): State => ({ kind: "error", message: errorMessage(cause) }),
  );
}

/** The songs in a shelf, without the `kind` tag. */
function songsOf(items: ShelfItem[]): Song[] {
  return items.flatMap((item) =>
    item.kind === "song"
      ? [
          {
            video_id: item.video_id,
            title: item.title,
            artists: item.artists,
            thumbnail_url: item.thumbnail_url,
          },
        ]
      : [],
  );
}

export function HomeScreen() {
  const [state, setState] = useState<State>({ kind: "loading" });
  const [playError, setPlayError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void fetchHome().then((next) => {
      if (active) setState(next);
    });
    return () => {
      active = false;
    };
  }, []);

  function retry() {
    setState({ kind: "loading" });
    void fetchHome().then(setState);
  }

  async function signOut() {
    try {
      await commands.signOut();
    } catch (cause) {
      setState({ kind: "error", message: errorMessage(cause) });
    }
  }

  /** Plays the shelf's songs, starting with `songs[index]`. */
  async function play(songs: Song[], index: number) {
    setPlayError(null);
    try {
      await commands.playQueue(songs, index);
    } catch (cause) {
      setPlayError(errorMessage(cause));
    }
  }

  return (
    <main className="screen">
      <header className="toolbar">
        <h1>Home</h1>
        <button type="button" onClick={signOut}>
          Sign out
        </button>
      </header>
      {playError && <p role="alert">{playError}</p>}
      {state.kind === "loading" && <p>Loading…</p>}
      {state.kind === "error" && (
        <div role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={retry}>
            Retry
          </button>
        </div>
      )}
      {state.kind === "loaded" &&
        (state.page.sections.length === 0 ? (
          <p>No sections.</p>
        ) : (
          <ul aria-label="Home sections" className="sections">
            {state.page.sections.map((section, index) => {
              const songs = songsOf(section.items);
              return (
                <li key={`${index}-${section.title}`}>
                  <h2>
                    {section.title} ({section.items.length})
                  </h2>
                  {songs.length > 0 && (
                    <ul aria-label={`${section.title} songs`} className="songs">
                      {songs.map((song, songIndex) => (
                        <li key={`${songIndex}-${song.video_id}`}>
                          <button type="button" onClick={() => void play(songs, songIndex)}>
                            {song.title}
                            {song.artists.length > 0 &&
                              ` · ${song.artists.map((artist) => artist.name).join(", ")}`}
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        ))}
    </main>
  );
}
```

Replace `app/src/App.tsx` with:

```tsx
import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { HomeScreen } from "./home/HomeScreen";
import { PlayerBar } from "./player/PlayerBar";

export function App() {
  const status = useAuthStatus();
  if (status === null) {
    return (
      <main className="screen">
        <p>Loading…</p>
      </main>
    );
  }
  if (status === "signed_in") {
    return (
      <>
        <HomeScreen />
        <PlayerBar />
      </>
    );
  }
  return <SignInScreen expired={status === "expired"} />;
}
```

Replace `app/src/styles.css` with:

```css
:root {
  font-family: system-ui, sans-serif;
  color-scheme: light dark;
}

body {
  margin: 0;
}

.screen {
  max-width: 720px;
  margin: 0 auto;
  padding: 32px 24px;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sections,
.songs {
  list-style: none;
  padding: 0;
}

.sections h2 {
  font-size: 1.1rem;
  margin: 24px 0 8px;
}

.songs button {
  width: 100%;
  padding: 6px 8px;
  text-align: left;
  background: none;
  border: none;
  border-radius: 6px;
  font: inherit;
  color: inherit;
  cursor: pointer;
}

.songs button:hover {
  background: color-mix(in srgb, currentColor 10%, transparent);
}

/* Leaves room for the fixed player bar. */
.screen:has(+ .player-bar) {
  padding-bottom: 140px;
}

.player-bar {
  position: fixed;
  right: 0;
  bottom: 0;
  left: 0;
  display: grid;
  gap: 6px;
  padding: 10px 24px;
  background: Canvas;
  border-top: 1px solid color-mix(in srgb, CanvasText 20%, transparent);
}

.player-track {
  display: flex;
  gap: 12px;
  align-items: baseline;
  min-width: 0;
}

.player-track span {
  opacity: 0.7;
}

.player-controls {
  display: flex;
  gap: 8px;
}

.player-controls button[aria-pressed="true"] {
  font-weight: 600;
}

.player-progress {
  display: flex;
  gap: 8px;
  align-items: center;
  font-variant-numeric: tabular-nums;
}

.player-progress input {
  flex: 1;
}

.player-ad {
  padding: 0 6px;
  border-radius: 4px;
  background: #f5c518;
  color: #000;
  font-weight: 600;
}
```

- [ ] **Step 5: Run the checks**

Run (from `app/`): `pnpm test && pnpm lint && pnpm typecheck`
Expected: `Tests  21 passed (21)` in 3 files; lint and typecheck clean.

- [ ] **Step 6: Commit and push**

```bash
git add app/src/player/ app/src/home/HomeScreen.tsx app/src/App.tsx app/src/App.test.tsx app/src/styles.css
git commit -m "feat(app): add player bar and play songs from home"
git push origin main
gh run list --limit 4
```

Wait for the `App` and `Rust` runs on this commit (`gh run watch <run-id> --exit-status`). Expected: green on macOS and Windows, and the Windows `App` run uploads `kaset-windows-nsis`.

---

### Task 7: ADR-0044, ADR-0043 amendments, AGENTS.md

**Files:**
- Create: `docs/adr/0044-tauri-playback-bridge.md`
- Modify: `docs/adr/0043-tauri-app-session-and-ipc.md`, `AGENTS.md` (the `## Tauri port (in progress)` section only)

**Interfaces:**
- Consumes: the decisions list at the top of this plan; names from Tasks 1–6.
- Produces: documentation only.

- [ ] **Step 1: Write ADR-0044**

Create `docs/adr/0044-tauri-playback-bridge.md`:

```markdown
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
  attempts), reports `progress` (at most 4 Hz), `ended`, and `failed`, and
  detects ads from the player classes and `getPresentingPlayerType(true) === 2`.
  After a pause command or an ended song it holds playback so the page's own
  autoplay cannot start another song. Rust drives it with `eval` of three fixed
  calls (`play`, `pause`, `seek(<number>)`).
- **IPC hardening:** capability `player-remote` grants
  `https://music.youtube.com/*` in the `player` window only `playback_event`, no
  core or event permissions. The command also checks the calling window label
  and URL. `PlaybackEvent` is a tagged enum with fixed fields and
  `deny_unknown_fields`; its video id is validated while deserializing. Events
  for any video other than the current song are ignored. Every video id is
  checked against `^[A-Za-z0-9_-]{11}$` before it reaches a URL, and songs with
  invalid ids are dropped from queues. The player window has its own navigation
  allowlist (https on YouTube, Google, and Google media/ad hosts, country Google
  domains, `about:`/`blob:`/`data:` frames); blocked navigations log scheme and
  host only.
- **Queue and state:** Rust owns the queue (`player::queue`) and the player
  state machine (`player::engine`), both pure and unit-tested. Repeat is
  off/all/one; shuffle keeps the current song first and permutes the rest with
  a clock-seeded xorshift (no dependency). An ended song advances the queue;
  repeat one reloads it; the end of the queue holds the page in `ended`.
  Previous restarts the song after three seconds. `player://state` is emitted on
  every status, track, or ad change and at most every 250 ms for progress;
  `player://queue` on queue or position changes.
- **OS media controls:** `souvlaki` 0.9.3. Its handle is created, updated, and
  dropped only on the main thread (`run_on_main_thread` plus a thread-local), as
  macOS `MPRemoteCommandCenter` and Windows SMTC (bound to the main window's
  HWND) require. Media events are handed to the async runtime before touching
  the player. Metadata is sent on track changes; playback state on status
  changes and seeks only, since the OS advances a playing position itself.
- **Session hardening carried from M2 reviews:** a session epoch (bumped by
  restore, sign-in, and sign-out) lets login completion and API errors that
  started under an older session be discarded (`sign_in_if`,
  `mark_expired_if`); the jar that expired cannot sign in again; API commands
  wait up to five seconds for the launch cookie refresh; WebView cookie reads
  and keyring I/O run on the blocking pool; cookie refresh also runs after each
  player page load. Details are in ADR-0043.

## Consequences

### Positive

- Playback, queue, and media keys work without the UI ever touching cookies or
  the remote page reaching more than one validated command.
- The queue and state machine are covered by plain `cargo test`.

### Negative

- Each song is a full page load: slower track changes than the Swift app's
  in-page router, and no gapless playback (planned later).
- The init script relies on undocumented YouTube Music page details (`<video>`,
  player classes, `getPresentingPlayerType`), which can change without notice.
- WebKit and WebView2 may publish their own media session for the page next to
  souvlaki's; the human checklist verifies which one the OS shows.
- `souvlaki` pulls in `block` 0.1.6 on macOS, which triggers a future
  incompatibility warning in Rust.
```

- [ ] **Step 2: Amend ADR-0043**

Replace `docs/adr/0043-tauri-app-session-and-ipc.md` with:

```markdown
# ADR-0043: Session storage and IPC typing in the Tauri app

## Status

Accepted (2026-11-22, milestone M2 of the Tauri port, ADR-0042). Amended in M3
(ADR-0044): two-slot keyring saves, session epochs, expired-jar rejection, the
launch gate, and cookie refresh from the player window.

## Context

M2 adds sign-in to the Tauri app (`app/`). The Rust side reads Google cookies
from the WebView, keeps them for `ytm-core`, and persists them across launches.
The React UI must never see cookies and must call Rust only through generated,
typed bindings. The M0 spike (`docs/superpowers/spikes/2026-11-22-tauri-playback.md`)
showed that `Webview::cookies_for_url` misses `.youtube.com` cookies on macOS and
that sign-in works with a Safari user agent.

## Decision

- **Types across the boundary:** `ytm-core` has an optional `specta` feature that
  derives `specta::Type` on its models; the app enables it. `ytm-core` stays free
  of Tauri. `tauri-specta` (2.0.0-rc.29, specta =2.0.0-rc.28) generates
  `app/src/bindings.ts` from a unit test (`cargo test -p kaset bindings_are_current`);
  CI fails when the committed file is stale. Commands reject with
  `AppError { kind, message }` (tauri-specta's default throw mode).
- **Cookies:** read with `Webview::cookies()` and keep domains `youtube.com` and
  its subdomains. `Webview::cookies_for_url` misses parent-domain cookies on
  macOS. For each cookie name, keep the most specific normalized
  domain, using the first cookie when domain lengths tie, and sort the header
  by name. `ytm-core`'s `InnerTubeClient` reads cookies from a shared
  `SharedCookies` handle on every request. Cookie reads run on Tauri's blocking
  pool, never on the main thread or an async worker.
- **Persistence:** the `Cookie` header goes to the OS secure store through
  `keyring` 4 under service `com.sertacozercan.kaset.desktop` (also the bundle
  identifier, distinct from the Swift app's). Windows Credential Manager caps a
  blob at 2560 bytes, so the header is split into 2000-byte chunks. Saves
  alternate between two slots, `session.a.N` and `session.b.N`; a save writes
  the slot that is not committed, then commits by writing `session.current`
  (`a:<count>` or `b:<count>`), then removes the other slot's chunks, so an
  interrupted save leaves the previous session readable. The M2 layout
  (`session.0..N` with `session.count`) is still read and is migrated by the
  next save. Loads reject recorded counts above 64; `clear` caps them at 64,
  deletes every slot's chunks highest index first and stops at the first
  failure, and removes `session.current` and `session.count` last, so a retry
  finds every remaining chunk. Keyring I/O runs on the blocking pool.
  Debug builds may set `KASET_SESSION_STORE=file` to use
  `<app data>/dev-session.txt` (0600) instead,
  because unsigned dev binaries make the Keychain prompt after every rebuild;
  release builds ignore the variable.
- **States:** `signed_out`, `signed_in`, `expired`, broadcast on `auth://changed`.
  An API request moves `signed_in` to `expired` on HTTP 401/403 or when its
  cookie jar is present without a SAPISID. A new sign-in replaces the retained
  cookies. Signed-out mode shows only the sign-in prompt. One transition lock
  serializes session transitions, including store I/O and shared-cookie updates.
- **Epochs:** a session epoch changes on restore, sign-in, and sign-out (not on
  rotation or expiry). Login completion captures it when the music.youtube.com
  page finishes loading and signs in only if it is unchanged; an API command
  captures it before its request and expires the session only if it is
  unchanged. A sign-out during a pending login, or a late 401 from an older
  session, therefore changes nothing.
- **Expired jar:** expiry remembers the cookies in use. A sign-in whose auth
  cookies (the rotation list below) equal them is rejected and the login window
  stays open, so a WebView that still holds the expired session cannot loop
  between `signed_in` and `expired`. A successful sign-in, sign-out, or restore
  forgets the expired jar.
- **Sign-out** stops playback and closes the player window, clears memory and
  the store, then clears cookies for `youtube.com`, `google.com`, and country
  Google domains such as `google.co.kr`, including their subdomains, from the
  shared WebView store. It emits `auth://changed` after the WebView cleanup
  attempt returns. Cleanup makes up to three passes and logs failures.
- **Rotation:** `refresh` runs at launch and after every player page load on
  music.youtube.com. It updates shared cookies when the user is already signed
  in, the WebView jar has a SAPISID, and the jar differs from the current one.
  It persists only when the set of name/value pairs for these auth cookies
  changes: `SID`, `HSID`, `SSID`, `APISID`, `SAPISID`, `__Secure-1PAPISID`,
  `__Secure-3PAPISID`, `__Secure-1PSID`, `__Secure-3PSID`. API commands wait up
  to five seconds for the launch refresh before their first request, so stale
  stored cookies do not expire a session the WebView has already rotated.
- **Hardening:** `withGlobalTauri: false`; a CSP for the local UI; the `main`
  window has the UI capability, the `player` window's music.youtube.com pages
  may call only `playback_event` (ADR-0044), and the login window can call
  nothing. The login
  window permits HTTPS on `google.com`, `youtube.com`, `gstatic.com`,
  `googleusercontent.com`, `googleapis.com`, `youtube-nocookie.com`, and country
  Google domains of the form `google.<cc>`, `google.co.<cc>`, or
  `google.com.<cc>` with a two-letter ASCII country code. It permits subdomains
  of all those hosts and the `about:`, `blob:`, and `data:` schemes for frames.
  Blocked navigations log only the scheme and host.
- **Windows tests:** `build.rs` passes MSVC linker arguments to embed the
  Common-Controls manifest in test executables.

## Consequences

### Positive

- The UI cannot reach cookies, and every IPC call is type-checked.
- Sessions survive restarts on macOS and Windows without a size failure, and an
  interrupted save no longer corrupts the stored session.
- `ytm-core` remains usable by the CLI (milestone 6) without Tauri.

### Negative

- tauri-specta and specta are release candidates pinned with `=`; upgrades must
  move both together.
- Keychain items are split across several entries; external tools see
  `session.current` and `session.a.N`/`session.b.N` items instead of one.
- The login navigation allowlist may block a legitimate Google sign-in hop that
  is not covered yet; the warning log names the host to add.
- After expiry, a WebView that still holds the expired session shows
  music.youtube.com in the login window; the user signs in again from the
  page's account menu.
```

- [ ] **Step 3: Update AGENTS.md**

In `AGENTS.md`, replace the whole `## Tauri port (in progress)` section (from that heading down to, not including, the next `## ` heading or the end of the file) with:

```markdown
## Tauri port (in progress)

The app is being rewritten as a Tauri 2 + Rust app (ADR-0042) on `main`. Until
the Swift sources are deleted, both trees exist.

- Rust workspace: `crates/ytm-core` (InnerTube client, parsers; no Tauri
  dependency, optional `specta` feature), `crates/ytm-explorer` (read-only API
  probes, replaces `Sources/APIExplorer` for Rust work), and `app/src-tauri`
  (crate `kaset`, the Tauri shell).
- Rust checks: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- App (`app/`, pnpm): `pnpm install`, `pnpm tauri dev`, `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm tauri build`.
- IPC: commands and events go through `tauri-specta`. After changing a command,
  event, or a type they use, run `cargo test -p kaset bindings_are_current` and
  commit `app/src/bindings.ts`. Never hand-write `invoke("…")` in the frontend.
  Event accessors are named after the event name (`player://state` →
  `events.playerState`). New commands also go in `build.rs`'s command list and
  a capability; commit the generated `permissions/autogenerated/*.toml`.
- Session storage (ADR-0043): OS secure store via `keyring`. For local debug
  builds, `KASET_SESSION_STORE=file pnpm tauri dev` stores the session in
  `dev-session.txt` in the app data directory instead of the Keychain (debug
  builds only).
- Read WebView cookies with `Webview::cookies()` plus a domain filter, never
  `cookies_for_url`. Run `Webview::cookies()`, `delete_cookie()`, and keyring
  I/O on the blocking pool (`webview_session::blocking`), never on the main
  thread or directly on an async worker.
- Work that can outlive a session change (login completion, API requests)
  captures `Session::epoch()` first and uses `sign_in_if` / `mark_expired_if`.
- Playback (ADR-0044): the hidden `player` window's music.youtube.com pages may
  call only `playback_event`. Never give the `player-remote` capability more
  permissions, never interpolate strings into `eval` scripts, and validate
  video ids with `player::video_id::VideoId` before building URLs.
- OS media controls (`souvlaki`) are touched only on the main thread through
  `AppHandle::run_on_main_thread`.
- Credentials stay in `ytm_core::secret::Secret`; never log cookie values or
  format `keyring::Error`.
- The Swift-era minimal-dependency rule does not apply to the Rust/Tauri code.
```

- [ ] **Step 4: Check for credential leaks**

Run:
```bash
git diff -- AGENTS.md docs/adr/ | grep -nE "SAPISID=[A-Za-z0-9_-]{20,}|__Secure-[0-9]PSID=[A-Za-z0-9]" || echo "no credential-like values"
```
Expected: `no credential-like values`.

- [ ] **Step 5: Commit and push**

```bash
git add docs/adr/0044-tauri-playback-bridge.md docs/adr/0043-tauri-app-session-and-ipc.md AGENTS.md
git commit -m "docs: record Tauri playback bridge decisions (ADR-0044)"
git push origin main
```

---

## Human runtime checklist (after Task 7; not a Codex task)

Codex workers cannot sign in or watch the desktop. The human runs this with the YouTube Music Premium account and records results (statuses and host names only, never cookie values) in `docs/superpowers/spikes/2026-11-22-tauri-playback.md` under a new "M3 app" heading.

**macOS** (from `app/`, `KASET_SESSION_STORE=file pnpm tauri dev`, signed in from M2 or sign in now):

1. Home lists shelves with song buttons. Click a song in "Quick picks" → within a few seconds audio plays, no window appears, the player bar shows the title, artist, `1 of N`, and the position advances about four times a second.
2. Pause / Play, Next, Previous (after 3 s it restarts the song; before 3 s it goes back), drag the seek slider and release → audio follows each.
3. Let a song finish → the next song in the shelf starts by itself; on the last song the bar shows Play and audio stops (no unrelated autoplay song starts).
4. Repeat all on the last song → wraps to the first; Repeat one → the same song restarts; Shuffle → `1 of N` stays on the current song and Next picks another song from the shelf.
5. Control Center → Now Playing shows the title, artist, and artwork; its play/pause, next, previous, and scrubber work. The keyboard's play/pause, next, and previous keys work while another app is focused. Note whether a second Now Playing entry (WebKit's own) appears.
6. Terminal: no `blocked player navigation` warning during playback (if one appears, note the host). A `refreshed session cookies from the WebView reason="player"` line may appear after a page load.
7. Sign out while playing → audio stops immediately, the sign-in screen appears. Sign in again → home loads.
8. Close the main window while playing → the app quits and audio stops.
9. Optional expiry check: none needed unless a 401 occurs; if the session expires, click "Sign in with Google" and confirm the login window either shows Google's sign-in or music.youtube.com (in that case use the page's account menu to sign in again) and does not flip between screens.
10. Repeat 1–3 without `KASET_SESSION_STORE` (Keychain path) and confirm `security find-generic-password -s com.sertacozercan.kaset.desktop -a session.current` finds the item (never add `-w`/`-g`). Sign out and confirm the same command reports that the item could not be found.

**Windows** (install `kaset-windows-nsis` from the latest green `App` run on hardware):

11. Sign in (spike risk 4 on WebView2), then repeat 1–4. This also answers spike risks 2 (autoplay without a gesture) and 3 (Premium/Widevine in WebView2): record whether audio plays and whether YouTube Music shows Premium (no ads).
12. The Windows media overlay (volume flyout / SMTC) shows Kaset with title and artwork; hardware media keys work. Note whether a second SMTC session from WebView2 appears.
13. Repeat 7 and 8.

## Self-review

- **Spec coverage (M3):** hidden player WebView sharing the session cookie store (Task 4 `window::load`, same WebView data directory); init script with `<video>` events, minimal ad detection, `video.play()` with retry (Task 4 `player.js`); remote capability granting exactly `playback_event` to `https://music.youtube.com` (Task 4 `player.json`); typed bounded payload, other-videoId rejection, videoId validation, player navigation allowlist (Tasks 3, 4); Rust `PlayerState` + queue with play, pause, next, previous, seek, set queue from songs, advance on ended, repeat and shuffle (Task 3) and their commands (Task 4); `player://state` throttled to ~4 Hz and `player://queue` (Tasks 3, 4); souvlaki OS media controls with main-thread handling (Task 5); typed commands and bindings (Task 4); minimal React player bar wired to home songs (Task 6); CI on macOS and Windows (Tasks 5, 6 watch the runs; no workflow change needed — `app.yml` and `rust.yml` already cover `app/**`, `Cargo.lock`, and the bindings drift check). M2 deferred items: see the table above. Not in M3 (spec "later"): router navigation, Up Next sync, gapless playback, the CLI remote.
- **Placeholder scan:** every code step has complete code; no TBD/TODO.
- **Type consistency:** `Session::{epoch, sign_in_if, mark_expired_if}` (Task 2) used by `login.rs` and `commands.rs` (Task 2); `PlayerService::update` signature (Task 4) used by player commands, `sign_out` (Task 4) and `media::attach` (Task 5); `Outcome`/`WebCommand`/`PlayerState`/`QueueState` (Task 3) used by `service.rs`, `events.rs`, `media.rs`; TS names `playQueue`, `togglePlayback`, `nextTrack`, `previousTrack`, `seekTo`, `setRepeat`, `setShuffle`, `playerSnapshot`, `events.playerState`, `events.playerQueue` match the bindings generated by tauri-specta rc.29.
- **Verified before writing:** all code in Tasks 1–6 was applied in order to a scratch copy of this repository at `ced7fb9` and checked after each task: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy -p ytm-core --all-targets -- -D warnings`; `cargo test --workspace` (kaset 50 → 58 → 93 → 98 → 103 tests; ytm-core +1); `node --check` on `player.js`; `pnpm lint`, `pnpm typecheck`, `pnpm test` (21 tests); and `pnpm tauri build --bundles app` produced `Kaset.app` on macOS (which validates both capability files and the generated permissions). External APIs were read in the registry sources: tauri 2.12.7 (`WebviewWindowBuilder::{visible, focused, skip_taskbar, background_throttling, initialization_script, on_navigation, on_page_load, user_agent}`, `WebviewWindow::{navigate, eval, close, cookies, hwnd}`, `AppHandle::run_on_main_thread`, `async_runtime::spawn_blocking` returning `tauri::Result<T>`, `emit_to` target matching, `specta::FunctionArg` for `Webview`), tauri-utils `BackgroundThrottlingPolicy` and `CapabilityRemote { urls }`, wry 0.57.3 (WebView2 adds `--autoplay-policy=no-user-gesture-required` only when no custom args are given), tauri-specta `Event::emit_to`, souvlaki 0.9.3 (macOS and Windows backends). **Not verified locally:** the `#[cfg(windows)]` branch of `media::attach` (types checked against windows 0.62.2's `HWND(pub *mut c_void)`; compiled by Windows CI in Task 5), and all runtime behaviour (left to the human checklist).
