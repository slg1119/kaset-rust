# AGENTS.md

Kaset is a YouTube Music desktop app for macOS and Windows. Tauri 2 hosts the
Rust application and a React, TypeScript, and Vite frontend.

## Rules

- **Keep real credentials out of everything.** Cookies, SAPISID values, auth tokens, and API keys must stay out of code, comments, logs, docs, test fixtures, and output. Use placeholders such as `"REDACTED"`, `"mock-token"`, or `"test-cookie"`.
- **Ask before running UI tests.** They launch the app and take over the desktop.
- **Prefer the API client over WebView.** Use the InnerTube client in `crates/ytm-core` for data requests. Use WebViews for sign-in and DRM playback.
- **Extend the API explorer.** Put reusable API probes in `crates/ytm-explorer`.
- **Keep standard macOS shortcuts.** Preserve `⌘M`, `⌘W`, `⌘Q`, `⌘H`, and `⌘,` unless the human asks to change them. Follow macOS and Apple Music conventions for media shortcuts and update `docs/keyboard-shortcuts.md`.
- **Record significant design changes** as an ADR in `docs/adr/`.

## Working Style

- Deliver what was asked, at the scope intended. Make routine judgment calls yourself, and check in only when different readings of the request would lead to materially different work.
- Match existing implementations: a new detail view follows the existing detail views, a new parser follows `crates/ytm-core/src/parsers/`.
- Adapt new code to existing public signatures, protocols, and model types instead of changing them, unless the task calls for it.
- If an approach goes wrong, revert and re-scope rather than layering patches on it.

## Build & Code Quality

From the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

From `app/`:

```bash
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm tauri dev
pnpm tauri build
```

Run the development and bundle commands when you need the app or installer.
The verification commands above run without a desktop app session.

## API Discovery

YouTube Music responses are undocumented. Before changing a request or parser,
verify the response shape with `cargo run -p ytm-explorer -- <command>` and
follow the `api-exploration` skill. Use read-only probes and keep any
`KASET_COOKIE_FILE` outside the repository. Exercise account writes against
mock servers. Get explicit approval before sending a live account write.

## Debugging & Measurement

For lifecycle, timing, or async ordering issues, capture timestamped state
transitions and reproduce before changing behavior. Measure again after the
fix and remove temporary instrumentation. Keep credentials and account data
out of traces.

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

## Architecture & Rules

- Rust workspace: `crates/ytm-core` (InnerTube client and parsers, optional
  `specta` feature), `crates/ytm-explorer` (read-only API probes), and
  `app/src-tauri` (crate `kaset`, the Tauri shell). `crates/kaset-cli` builds
  the standalone CLI, `crates/kaset-session` shares the session store between
  app and CLI, and `crates/kaset-ipc` defines their local playback protocol.
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
- Credentials stay in `ytm_core::secret::Secret`. Never log cookie values or
  format `keyring::Error`.
