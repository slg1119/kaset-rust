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
  its subdomains. `Webview::cookies_for_url` misses parent-domain cookies on
  macOS. For each cookie name, keep the most specific normalized
  domain, using the first cookie when domain lengths tie, and sort the header
  by name. `ytm-core`'s `InnerTubeClient` reads cookies from a shared
  `SharedCookies` handle on every request.
- **Persistence:** the `Cookie` header goes to the OS secure store through
  `keyring` 4 under service `com.sertacozercan.kaset.desktop` (also the bundle
  identifier, distinct from the Swift app's). Windows Credential Manager caps a
  blob at 2560 bytes, so the header is split into 2000-byte entries
  `session.0..N` with `session.count` written last. Store `clear` deletes chunks
  before removing the count, sweeping contiguous orphaned chunks from zero when
  the count is missing or corrupt, or stale chunks starting at the recorded
  count. Each sweep stops at the first missing entry or after 64 chunks.
  Debug builds may set `KASET_SESSION_STORE=file` to use
  `<app data>/dev-session.txt` (0600) instead,
  because unsigned dev binaries make the Keychain prompt after every rebuild;
  release builds ignore the variable.
- **States:** `signed_out`, `signed_in`, `expired`, broadcast on `auth://changed`.
  An API request moves `signed_in` to `expired` on HTTP 401/403 or when its
  cookie jar is present without a SAPISID. A new sign-in replaces the retained
  cookies. Signed-out mode shows only the sign-in prompt. One transition lock
  serializes session transitions, including store I/O and shared-cookie updates.
- **Sign-out** clears memory and the store, then clears cookies for
  `youtube.com`, `google.com`, and country Google domains such as `google.co.kr`,
  including their subdomains, from the shared WebView store. It emits
  `auth://changed` after the WebView cleanup attempt returns. Cleanup makes up to
  three passes and logs failures.
- **Rotation:** at launch, `refresh` updates shared cookies when the user is
  already signed in, the WebView jar has a SAPISID, and the jar differs from
  the current one. It persists only when the set of name/value pairs for
  these auth cookies changes: `SID`, `HSID`, `SSID`, `APISID`, `SAPISID`,
  `__Secure-1PAPISID`, `__Secure-3PAPISID`, `__Secure-1PSID`, `__Secure-3PSID`.
- **Hardening:** `withGlobalTauri: false`; a CSP for the local UI; only the
  `main` window has a capability (the login window can call nothing); the login
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
- Sessions survive restarts on macOS and Windows without a size failure.
- `ytm-core` remains usable by the CLI (milestone 6) without Tauri.

### Negative

- tauri-specta and specta are release candidates pinned with `=`; upgrades must
  move both together.
- Keychain items are split across several entries; external tools see
  `session.N` items instead of one.
- The login navigation allowlist may block a legitimate Google sign-in hop that
  is not covered yet; the warning log names the host to add.
