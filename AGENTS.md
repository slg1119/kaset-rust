# AGENTS.md

Guidance for AI coding agents working in this repository.

Kaset is a native macOS client for YouTube Music and YouTube, written in Swift 6 and SwiftUI. The deployment target is macOS 15.4, with macOS 26 features such as Liquid Glass layered on top. Write idiomatic SwiftUI and Swift Concurrency code that follows Apple's Human Interface Guidelines.

## Rules

- **Keep real credentials out of everything.** Cookies, SAPISID values, auth tokens, and API keys must not appear in code, comments, logs, docs, test fixtures, or your output. Use placeholders such as `"REDACTED"`, `"mock-token"`, or `"test-cookie"`. The repo is public and the app holds signed-in Google sessions, so a leaked value compromises a real account.
- **Ask before running UI tests.** They launch the app and take over the desktop.
- **Ask before adding a third-party dependency.** The project keeps them minimal; Sparkle, for app updates, is currently the only one.
- **Prefer the API clients over WebView.** Use `YTMusicClient` (YouTube Music) or `YouTubeClient` (YouTube) whenever the endpoint exists. WebView is only for DRM playback and sign-in.
- **Extend the API explorer instead of writing one-off scripts.** API exploration and debugging tooling belongs in `Sources/APIExplorer/main.swift` so the next person can reuse it.
- **Keep standard macOS shortcuts.** Don't rebind `⌘M`, `⌘W`, `⌘Q`, `⌘H`, or `⌘,` unless the human asks. For media shortcuts, follow macOS and Apple Music conventions and update `docs/keyboard-shortcuts.md`.
- **Record significant design changes** as an ADR in `docs/adr/`.

## Working Style

- Deliver what was asked, at the scope intended. Make routine judgment calls yourself, and check in only when different readings of the request would lead to materially different work.
- Match existing implementations: a new detail view follows the existing detail views, a new parser follows `Sources/Kaset/Services/API/Parsers/`.
- Adapt new code to existing public signatures, protocols, and model types instead of changing them, unless the task calls for it.
- If an approach goes wrong, revert and re-scope rather than layering patches on it.

## Build & Code Quality

```bash
swift build                              # Build
swift test --skip KasetUITests --no-parallel  # Unit tests, serial like CI (UI tests live in a separate Xcode project)
swiftlint --strict && swiftformat .      # Lint & format
```

A code change is done when these pass for the code you touched. Stay CLI-first; use Xcode or `xcodebuild` only for UI tests, runtime debugging, screenshots, or scheme-specific investigation.

CI pins the SwiftFormat version in `.github/workflows/lint.yml`; format with that version, since newer releases can rewrite unrelated files.

`.swiftformat` uses `--self insert`: in static methods call other static methods as `Self.methodName()`, and in instance methods write `self.property` explicitly.

## Coding Rules

Project rules that differ from standard Swift/SwiftUI conventions and aren't mechanically checked:

| Avoid | Use | Why |
|-------|-----|-----|
| `.background(.ultraThinMaterial)` | `compatGlass(...)` helpers in `LiquidGlassCompat.swift`, or `.glassEffect()` behind `#available(macOS 26.0, *)` | Liquid Glass on macOS 26 with a macOS 15 fallback |
| Force unwraps (`!`) | Optional handling or `guard` | Project policy |

- Mark `@Observable` classes with `@MainActor`.
- Write new unit tests with Swift Testing (`@Test`, `#expect`).
- Throw `YTMusicError.authExpired` on HTTP 401/403.
- Use `.task` instead of `.onAppear { Task { } }`.

`swiftlint --strict` enforces the mechanically checkable bans (`print()`, `DispatchQueue`, `NavigationView`, `foregroundColor`, `cornerRadius`, `contentMargins`) and names the replacement in each message; see `.swiftlint.yml`.

## Localization

`Sources/Kaset/Resources/Localizable.xcstrings` is the source of truth. Packaged builds compile the catalog, but SwiftPM/Xcode runtime builds read the checked-in `Sources/Kaset/Resources/*.lproj/Localizable.strings` mirrors, so the two must change together or the build types diverge.

- When adding keys or changing translations, update the catalog first, then the matching `.lproj/Localizable.strings` files in the same change.
- Run `swift test --skip KasetUITests --filter LocalizationCatalogParityTests` after localization changes.
- Siri phrases for App Shortcuts live in `Sources/Kaset/Resources/AppShortcuts.xcstrings` (no `.lproj` mirror). `Scripts/build-app.sh` fails if it drifts from `KasetShortcuts`, so update it whenever a phrase changes.
- When adding a locale, also register its `.lproj` in `Package.swift`, add the `SettingsManager.ContentLanguage` case, and extend the localization tests.

## API Discovery

YouTube's response shapes are undocumented and change without notice, so a parser written from inference compiles and then silently returns nothing. Before adding or changing an API call, request shape, or response parser, verify the real request and response with `swift run api-explorer` (see the `api-exploration` skill). Prefer read-only probes. Get explicit approval from the human before sending a live request that changes account data.

## Debugging & Measurement

For bugs about timing, lifecycle, or "why didn't this run/load/update" (SwiftUI `.task` or state churn, cold-launch ordering, perceived latency), measure before fixing. Reasoning about SwiftUI lifecycle or async ordering from source alone is unreliable, and a short timestamped trace settles in one launch what hypothesizing cannot. Add the trace, reproduce, fix what the data points at, re-measure, then remove the instrumentation before committing.

The app is sandboxed, which breaks most ad-hoc logging without any error:

- `Logger`/`os_log` `.info` and `.debug` lines don't reliably appear in `log stream` or `log show`.
- Writes to a hardcoded `/tmp/...` path are blocked.
- Window-screenshot automation is unreliable.

Write throwaway traces to `NSTemporaryDirectory()`, `synchronize()` after each line, and read them from `~/Library/Containers/com.sertacozercan.Kaset/Data/tmp/` (`com.sertacozercan.Kaset.debug` for packaged debug builds). `docs/common-bug-patterns.md` has the trace template, the single-flight load pattern for the `.task`-restart cancellation deadlock, concurrency anti-patterns, and a pre-submit checklist.

### Cookie storage in debug builds

Sandboxed builds without a valid Developer ID or provisioning profile (local debug and ad-hoc builds) hang inside `SecItemCopyMatching` when they query the Keychain.

- `#if DEBUG` builds therefore store cookies in Application Support at `Kaset/cookies.dat` and skip the Keychain. Launch with `KASET_DEBUG_COOKIE_STORAGE=keychain` to test the production Keychain path.
- Packaged debug builds use the bundle ID `com.sertacozercan.Kaset.debug`, which gives them their own container, UserDefaults, and WebKit cookie jar. Don't package a debug build with the release bundle ID: each build's startup would overwrite the other's session.
- The Keychain is shared, so `KASET_DEBUG_COOKIE_STORAGE=keychain` reads and writes the release archive.
- The debug file backend has its own sign-out tombstone key (`authCookieBackupInvalidated.debugFileStorage`). Debug-only auth state must not write the release key.

## Code Review

Run the `autoreview` skill by default after committing non-trivial code changes and before pushing them. Skip it for docs-only or trivial changes, or when the human opts out. Finish the focused tests first, and don't run the review at the same time as tests: the CPU contention makes timing-sensitive tests flaky.

Use these Kaset settings with the skill, from the repository root:

```bash
python3 .agents/skills/autoreview/scripts/autoreview --mode branch --base origin/<base> --max-priority P3 \
  --source-context-file AGENTS.md \
  --prompt "Also report changes that violate the repository rules in the AGENTS.md source context, such as credential handling, API-over-WebView, localization parity, and the Coding Rules table."
```

`<base>` is the PR's base branch (`gh pr view --json baseRefName --jq .baseRefName`), or `main` when there is no PR yet. If that lookup fails for any other reason, such as auth or network, fix it or ask; don't assume `main`.

- `--max-priority P3` accepts findings at every priority (P0 through P3); the helper's default of P0 hides the rest in audit output.
- `--source-context-file AGENTS.md` gives the reviewer these repo rules as evidence, and `--prompt` asks it to check them; the helper treats source files as evidence, not instructions. `--source-context-file` only works in `branch` and `commit` modes; drop it when asked to review uncommitted work with `--mode local`.
- `branch` mode excludes uncommitted work, so format and commit your own changes first. For review-only requests, leave the checkout as it is.

Review findings are advisory, including P2 and P3: check each one against the real code path, fix the ones that hold up, and reject the rest only with a reason. After review-driven code changes, rerun the focused tests and the review once. When you report back, include the review command, the tests you ran, which findings you accepted or rejected and why, and the result of the final run.

`.agents/skills/autoreview/` is an unmodified copy of [`sozercan/skills`](https://github.com/sozercan/skills/tree/main/skills/autoreview) (commit `1b8ef21`), without that directory's own `AGENTS.md`/`CLAUDE.md`, which only govern maintenance of the source repository. Don't edit it in place: change the source repository, then replace the whole directory with the new version, again leaving out those two files.

## Skills

Repeatable, repo-specific workflows live in `.agents/skills/` (also exposed as `.claude/skills/`). Add new workflows there so this file stays focused on repo-wide rules.

## Tauri port (in progress)

The app is being rewritten as a Tauri 2 + Rust app (ADR-0042) on `main`. Until
the Swift sources are deleted, both trees exist.

- Rust workspace: `crates/ytm-core` (InnerTube client, parsers; no Tauri
  dependency, optional `specta` feature), `crates/ytm-explorer` (read-only API
  probes, replaces `Sources/APIExplorer` for Rust work), and `app/src-tauri`
  (crate `kaset`, the Tauri shell). `crates/kaset-cli` builds the standalone
  `kaset` CLI and `crates/kaset-session` holds its shared non-Tauri session store.
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
  builds only). Keyring saves alternate slots and commit through
  `session.current`. The M2 layout migrates on the next save. Counts are
  bounded at 64 chunks per slot.
- Read WebView cookies with `Webview::cookies()` plus a domain filter, never
  `cookies_for_url`. Run `Webview::cookies()`, `delete_cookie()`, and keyring
  I/O on the blocking pool (`webview_session::blocking`). Startup session
  restore is the one exception to keeping the main thread unblocked: Tauri
  `setup` waits for keyring I/O via `block_on` while the pool performs it.
- Work that can outlive a session change captures `Session::epoch()` before
  reading WebView cookies or starting an API request. Use `sign_in_if` for
  login completion, `refresh_if` for launch and player refresh, and
  `mark_expired_if` on the blocking pool for API expiry. The launch gate opens
  on completion or after the five-second timeout.
- Playback (ADR-0044): the hidden `player` window's music.youtube.com pages may
  call only `playback_event` through the `player-remote` capability's sole
  permission, `allow-playback-event`. Never interpolate strings into `eval`
  scripts. Validate video ids with `player::video_id::VideoId` before building
  URLs. Keep the command boundary limits of 500 songs and 2048 bytes per song
  or artist string. Use `PlayerService::stop_for_sign_out` to hold the player
  lock through cookie cleanup and session sign-out.
- OS media controls (`souvlaki`) are touched only on the main thread through
  `AppHandle::run_on_main_thread`.
- UI (ADR-0045): server data goes through TanStack Query (`app/src/query.ts`:
  `queryKeys`, stale times, retry policy); do not fetch in `useEffect`. Action
  errors go to the single notice area (`useNotice().report`), load errors stay
  in the screen with Retry. Navigation is state-based (`shell/Sidebar.tsx`
  `Route`); there is no router. Artwork goes through `music/Artwork.tsx`, which
  only loads https URLs on the hosts the CSP allows. The main window is
  transparent: screens paint `--bg`; only the sidebar is translucent, and only
  when `<html data-vibrancy="native">` (set from the `window_vibrancy`
  command). Every `backdrop-filter` needs the `-webkit-` prefix. New API commands use
  `signed_in_request` in `commands.rs`. Test screens with
  `test/render.tsx` (`renderWithProviders`, `renderApp`) and `mockCommands`.
- Search (`ytm-core::parsers::search`) is a port of the Swift parser verified
  with `cargo run -p ytm-explorer -- search "<query>" --parse` and a signed-in
  `KASET_COOKIE_FILE` outside the repository.
- Credentials stay in `ytm_core::secret::Secret`; never log cookie values or
  format `keyring::Error`.
- The Swift-era minimal-dependency rule does not apply to the Rust/Tauri code.
