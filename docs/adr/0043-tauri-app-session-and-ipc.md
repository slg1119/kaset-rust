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
  (`a:<count>` or `b:<count>`). An interruption before the pointer write leaves
  the previous session readable. After the pointer write, stale chunk cleanup
  is best effort. The app still reads the M2 layout (`session.0..N` with
  `session.count`) and migrates it on the next save. Saves and loads enforce a
  maximum of 64 chunks per slot, and
  `clear` caps recorded counts at 64. It deletes chunks highest index first,
  stops at the first failure, and removes `session.count` and `session.current`
  after deleting the chunks so a retry can find what remains. Keyring I/O runs
  on the blocking pool. Startup session restore is the one exception to keeping
  the main thread unblocked: Tauri `setup` waits for the restore via `block_on`
  while the pool performs the keyring I/O.
  Debug builds may set `KASET_SESSION_STORE=file` to use
  `<app data>/dev-session.txt` (0600) instead,
  because unsigned dev binaries make the Keychain prompt after every rebuild;
  release builds ignore the variable. The desktop app and the read-only CLI
  share the storage implementation in the Tauri-independent `kaset-session`
  crate.
- **States:** `signed_out`, `signed_in`, `expired`, broadcast on `auth://changed`.
  An API request can move `signed_in` to `expired` on HTTP 401/403 or when its
  cookie jar is present without a SAPISID. The expiry transition runs on the
  blocking pool. A new sign-in replaces the retained cookies. Signed-out mode
  shows only the sign-in prompt. One transition lock serializes session
  transitions, including store I/O and shared-cookie updates.
- **Epochs:** restore, a new sign-in, and sign-out advance the session epoch.
  Rotation and expiry preserve it. Login completion captures it when the
  music.youtube.com page finishes loading, before reading cookies, and calls
  `sign_in_if`. If the signed-in jar is identical, `sign_in_if` skips the save
  and epoch bump. The API command captures the epoch before its request and
  calls `mark_expired_if` on an auth error. Both methods discard stale work.
- **Expired jar:** expiry remembers the cookies in use. A sign-in whose auth
  cookies (the rotation list below) equal them is rejected with
  `SameAsExpired`. For the current epoch, recovery stops playback, attempts to
  clear session-domain WebView cookies, and navigates to the non-passive Google
  sign-in URL once per login window. If the same jar is rejected again, the
  window closes and the session stays expired. A successful new sign-in,
  sign-out, or restore forgets the expired jar.
- **Sign-out** closes the login window if open and stops playback and closes
  the player window under `PlayerService::stop_for_sign_out`. While holding the
  player lock, it attempts to clear cookies for `youtube.com`, `google.com`,
  and country Google domains such as `google.co.kr`, including subdomains,
  from the shared WebView store. It then clears memory, advances the epoch,
  and clears the persisted store. WebView cleanup makes up to three passes
  and logs failures. The command emits `auth://changed` with the current
  status even if the blocking task fails.
- **Rotation:** launch and each completed player page load on
  music.youtube.com capture the epoch before reading WebView cookies and
  call `Session::refresh_if(epoch, jar)`. It updates shared cookies when the
  epoch still matches, the user is signed in, the WebView jar has a SAPISID,
  and the jar differs from the current one.
  It persists only when the set of name/value pairs for these auth cookies
  changes: `SID`, `HSID`, `SSID`, `APISID`, `SAPISID`, `__Secure-1PAPISID`,
  `__Secure-3PAPISID`, `__Secure-1PSID`, `__Secure-3PSID`. Launch waits up to
  five seconds for refresh and opens the API gate on completion or timeout.
  The API command also bounds its wait for the gate at five seconds. A
  blocking refresh already in progress can finish after the timeout, with
  `refresh_if` still checking its epoch before applying the result.
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
- After expiry, a WebView holding the same auth cookies is redirected to
  interactive Google sign-in after a cleanup attempt.
