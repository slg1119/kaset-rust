# ADR-0043: Session storage and IPC typing in the Tauri app

## Status

Accepted (2026-11-22, milestone M2 of the Tauri port, ADR-0042).

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
  its subdomains; never `cookies_for_url`. `ytm-core`'s `InnerTubeClient` reads
  cookies from a shared `SharedCookies` handle on every request, so sign-in,
  sign-out, and rotation need no new client.
- **Persistence:** the `Cookie` header goes to the OS secure store through
  `keyring` 4 under service `com.sertacozercan.kaset.desktop` (also the bundle
  identifier, distinct from the Swift app's). Windows Credential Manager caps a
  blob at 2560 bytes, so the header is split into 2000-byte entries
  `session.0..N` with `session.count` written last. Debug builds may set
  `KASET_SESSION_STORE=file` to use `<app data>/dev-session.txt` (0600) instead,
  because unsigned dev binaries make the Keychain prompt after every rebuild;
  release builds ignore the variable.
- **States:** `signed_out`, `signed_in`, `expired`, broadcast on `auth://changed`.
  HTTP 401/403 moves `signed_in` to `expired` without deleting cookies; a new
  sign-in replaces them. Signed-out mode shows only the sign-in prompt.
- **Sign-out** clears memory and the store, then deletes every youtube.com and
  google.com cookie from the shared WebView store (up to three passes).
- **Rotation:** at launch, WebView cookies replace the stored session only when
  already signed in, so a sign-out is never undone.
- **Hardening:** `withGlobalTauri: false`; a CSP for the local UI; only the
  `main` window has a capability (the login window can call nothing); the login
  window's navigation is limited to https Google/YouTube hosts plus `about:`,
  `blob:`, `data:` frames, and blocked navigations log scheme and host only.

## Consequences

### Positive

- The UI cannot reach cookies, and every IPC call is type-checked.
- Sessions survive restarts on macOS and Windows without a size failure.
- `ytm-core` remains usable by the CLI (milestone 6) without Tauri.

### Negative

- tauri-specta and specta are release candidates pinned with `=`; upgrades must
  move both together.
- Keychain items are split across several entries; external tools see
  `session.N` items instead of one.
- The login navigation allowlist may block a legitimate Google sign-in hop that
  is not covered yet; the warning log names the host to add.
