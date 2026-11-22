# Kaset Tauri Port — M2 (Sign-in and Cookie Persistence) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Scaffold the Tauri 2 app (`app/`) and make sign-in work end to end: a Google login window, cookies read from the WebView, the session persisted in the OS secure store and restored at launch, sign-out, `AuthExpired` handling, and a minimal React UI that lists signed-in home section titles.

**Architecture:** `ytm-core` gains a runtime-swappable cookie handle (`SharedCookies`) and an optional `specta` feature so its models cross the Tauri boundary unchanged. `app/src-tauri` (crate `kaset`) owns a `Session` (pure Rust state machine over a `SessionStore` trait: chunked `keyring` store in production, file store opt-in for debug builds), a login window with a navigation allowlist, and four typed commands plus the `auth://changed` event, exported by `tauri-specta` to `app/src/bindings.ts`. React renders a sign-in screen or a home screen from `auth://changed`, and never sees cookies.

**Tech Stack (versions verified 2026-11-22 against crates.io/npm and the crate sources):**
- Rust: tauri **2.12.7** (pin 2.x — crates.io "latest" is 3.0.0-alpha.6, do not use it), tauri-build **2.7.6**, wry 0.57.3 (transitive), tauri-specta **=2.0.0-rc.29** (features `derive`, `typescript`; requires specta `=2.0.0-rc.28` and tauri `2`), specta **=2.0.0-rc.28** (feature `derive`), specta-typescript **=0.0.15**, keyring **4.2.0** (default feature `v1`: `keyring::Entry::new(service, user)`, `get_secret`/`set_secret`/`delete_credential`, `keyring::Error::NoEntry`), tokio 1 (`sync`), tracing-subscriber 0.3 (`env-filter`), tempfile 3 (dev).
- Frontend (npm, pnpm 10.29.1, Node 26 locally / 24 in CI): @tauri-apps/api **~2.12.5**, @tauri-apps/cli **~2.12.6**, react/react-dom ^19.3.1, vite ^8.3.8, @vitejs/plugin-react ^6.3.0, typescript **~5.9.3** (not 7.x: typescript-eslint 8.73 supports `<6.1`), vitest ^5.1.1, jsdom ^31.0.1, @testing-library/react ^16.3.3, @testing-library/jest-dom ^8.0.0, @testing-library/user-event ^14.6.13, eslint ^10.15.0, @eslint/js ^10.0.1, typescript-eslint ^8.73.0, eslint-plugin-react-hooks ^7.1.1, globals ^17.14.0.

**Spec:** `docs/superpowers/specs/2026-11-22-tauri-port-design.md` (milestone 2). Spike findings that shape this plan: `docs/superpowers/spikes/2026-11-22-tauri-playback.md`.

## Global Constraints

- No real credentials anywhere — code, comments, logs, fixtures, docs, output. Placeholders only: `test-sapisid`, `test-cookie`, `test-api-key`.
- Never print or log cookie values, SAPISID, or Authorization headers. Cookie headers live in `ytm_core::secret::Secret` (Debug prints `REDACTED`). Never format a `keyring::Error` with `{:?}` or `{}` — some variants hold the stored bytes; use the fixed descriptions from Task 4.
- `crates/ytm-core` must not depend on Tauri. `specta` is allowed only behind the optional `specta` feature.
- Tauri **2.x only**. Commands are exposed only through `tauri-specta` bindings in `app/src/bindings.ts`; no hand-written `invoke("…")` in the frontend.
- Read WebView cookies with `Webview::cookies()` plus a domain-suffix filter, never `cookies_for_url` (wry's macOS implementation matches the cookie domain to the URL host exactly, so `.youtube.com` cookies never match `music.youtube.com`).
- Call `Webview::cookies()` / `delete_cookie()` only from async code (async commands or `tauri::async_runtime::spawn`): it deadlocks on Windows from the main thread or a synchronous handler.
- `withGlobalTauri: false`. Only the `main` window gets a capability; the login window has none.
- Live requests to YouTube are read-only (`browse`, web-client page fetch).
- Do not modify or delete Swift sources (`Sources/`, `Tests/`, `Package.swift`) or `spikes/`. Do not run Swift UI tests.
- Rust edition 2024. For touched crates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` must pass. For `app/`: `pnpm lint`, `pnpm typecheck`, `pnpm test` must pass.
- Commit directly on `main` and `git push origin main` at the end of every task (the human asked for direct commits, no PRs). Stage only the files the task lists.

## Decisions made in this plan

Recorded in ADR-0043 by Task 9.

1. **specta:** optional `specta` feature in `ytm-core` derives `specta::Type` on `models.rs`; the app enables it. No DTO mirroring.
2. **Bindings:** exported by a unit test (`cargo test -p kaset bindings_are_current`), not at app start-up. CI fails if the committed `app/src/bindings.ts` differs. `ErrorHandlingMode::Throw` (tauri-specta's default): commands resolve to the success type and reject with the serialized `AppError` `{ kind, message }`. `disable_serde_phases()` so each Rust type exports as one TypeScript type.
3. **Bundle identifier / keyring service:** `com.sertacozercan.kaset.desktop` (distinct from the Swift app's `com.sertacozercan.Kaset`, so the two never share a container, WebView data store, or Keychain items during the port).
4. **Keyring layout:** Windows Credential Manager caps a credential blob at 2560 bytes and a signed-in youtube.com cookie header is usually larger, so the header is split into 2000-byte entries `session.0..N` plus `session.count` (written last).
5. **Dev store:** `KASET_SESSION_STORE=file` selects a plain file `<app_data_dir>/dev-session.txt` (mode 0600 on Unix) — **debug builds only**; release builds ignore the variable. Tauri dev builds are not sandboxed (unlike the Swift debug builds that hang in `SecItemCopyMatching`), but an unsigned binary makes the Keychain prompt after every rebuild.
6. **Persisted cookies:** all cookies whose domain is `youtube.com` or a subdomain, as one `Cookie` header string.
7. **Expiry:** HTTP 401/403 moves `SignedIn → Expired`, keeps the cookies and the stored copy (a new sign-in replaces them), and the UI shows the sign-in screen with an "expired" message. Signed-out mode is just the sign-in prompt (signed-out InnerTube requests in Korea return only a region message).
8. **Sign-out:** clears memory and the store, emits `auth://changed`, then deletes every youtube.com/google.com cookie from the shared WebView store (up to three passes, like the Swift cookie clearer).
9. **Cookie rotation:** at launch the app re-reads the WebView's youtube.com cookies and replaces the stored session only if the user is already signed in (never signs a user back in after sign-out). M3's player will call the same `Session::refresh`.
10. **Login navigation allowlist:** `https` on google.com, youtube.com, gstatic.com, googleusercontent.com, googleapis.com, youtube-nocookie.com and their subdomains, country Google domains (`google.co.kr`, `google.com.au`, `google.de`), plus `about:`, `blob:`, `data:` (wry on macOS asks the navigation handler for every frame, not only the main frame). Blocked navigations are logged with scheme and host only.
11. **Icons:** copied from the spike (Tauri defaults) as placeholders; the real icon comes with the M4 UI shell.
12. **TanStack Query** (spec) is deferred to M4: M2's one screen does not need caching.

## File Structure

```
Cargo.toml                                  # workspace: add "app/src-tauri" to members
crates/ytm-core/
  Cargo.toml                                # optional specta dependency + `specta` feature
  src/auth.rs                               # + CookieJar::from_pairs, PartialEq, SharedCookies
  src/client.rs                             # cookies: SharedCookies; with_shared_cookies, set_cookies, cookies()
  src/models.rs                             # cfg_attr(feature = "specta", derive(specta::Type))
  tests/client.rs                           # + runtime cookie swap tests
app/
  package.json, pnpm-lock.yaml              # pnpm scripts: dev, build, tauri, lint, typecheck, test
  index.html, vite.config.ts                # Vite on fixed port 1420; Vitest (jsdom) config
  tsconfig.json, tsconfig.app.json, tsconfig.node.json
  eslint.config.js, .gitignore
  src/main.tsx, src/styles.css
  src/bindings.ts                           # GENERATED by tauri-specta (committed)
  src/errors.ts                             # isAppError, errorMessage
  src/App.tsx, src/App.test.tsx             # status → SignInScreen | HomeScreen
  src/auth/useAuthStatus.ts                 # authStatus() + auth://changed
  src/auth/SignInScreen.tsx (+ .test.tsx)
  src/home/HomeScreen.tsx                   # get_home → section titles, retry, sign out
  src/test/setup.ts                         # jest-dom matchers, cleanup, clearMocks
  src-tauri/
    Cargo.toml, build.rs                    # crate `kaset`, lib `kaset_lib`; app manifest lists the 4 commands
    tauri.conf.json                         # identifier, withGlobalTauri false, CSP/devCsp
    capabilities/default.json               # main window only: core:event:default + 4 commands
    permissions/autogenerated/*.toml        # generated by tauri-build (committed)
    icons/                                  # copied from spikes/tauri-playback/src-tauri/icons
    .gitignore                              # /gen/schemas
    src/main.rs                             # calls kaset_lib::run()
    src/lib.rs                              # specta_builder(), run(), bindings export test
    src/error.rs                            # AppError { kind, message }, ErrorKind
    src/events.rs                           # AuthChanged (auth://changed)
    src/session/mod.rs                      # Session state machine
    src/session/status.rs                   # AuthStatus
    src/session/store.rs                    # SessionStore, ChunkedStore+KeyringBackend, FileStore
    src/session/cookies.rs                  # domain filters, youtube_cookie_jar, session_cookies
    src/state.rs                            # AppState { session, lazily built InnerTubeClient }
    src/login.rs                            # login window, navigation allowlist, sign-in completion
    src/commands.rs                         # auth_status, sign_in, sign_out, get_home
.github/workflows/rust.yml                  # paths + bindings drift check + featureless ytm-core clippy
.github/workflows/app.yml                   # new: pnpm checks + tauri build (macOS, Windows) + NSIS artifact
docs/adr/0043-tauri-app-session-and-ipc.md  # decisions above
AGENTS.md                                   # Tauri section: app commands, dev store, bindings
```

Task order matters: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Tasks 1–2 touch only `ytm-core`; Task 3 depends on Task 2 (the app enables `ytm-core/specta`).

---

### Task 1: ytm-core — swappable cookies and `CookieJar::from_pairs`

`InnerTubeClient` currently freezes `Option<CookieJar>` at construction. The app must sign in, sign out, and pick up rotated cookies without rebuilding the client, and it builds jars from WebView (name, value) pairs.

**Files:**
- Modify: `crates/ytm-core/src/auth.rs`
- Modify: `crates/ytm-core/src/client.rs`
- Test: `crates/ytm-core/src/auth.rs` (unit tests module), `crates/ytm-core/tests/client.rs`

**Interfaces:**
- Consumes: existing `ytm_core::auth::CookieJar { from_header(&str), header() -> &Secret, sapisid() -> Option<Secret> }`, `ytm_core::secret::Secret`, `InnerTubeClient::new(ClientConfig, Option<CookieJar>) -> Result<Self, YtmError>` (signature unchanged; `ytm-explorer` keeps compiling).
- Produces:
  - `CookieJar` now derives `PartialEq, Eq`.
  - `CookieJar::from_pairs<I, N, V>(pairs: I) -> CookieJar where I: IntoIterator<Item = (N, V)>, N: AsRef<str>, V: AsRef<str>` — joins `name=value` with `"; "`, trims both, skips empty names, names containing `=` or `;`, values containing `;`.
  - `pub struct SharedCookies` (`Clone + Default`, clones share one slot): `SharedCookies::new(Option<CookieJar>) -> Self`, `get(&self) -> Option<CookieJar>`, `set(&self, Option<CookieJar>)`; `Debug` prints `SharedCookies(set)` / `SharedCookies(empty)`.
  - `InnerTubeClient::with_shared_cookies(config: ClientConfig, cookies: SharedCookies) -> Result<Self, YtmError>`, `InnerTubeClient::cookies(&self) -> &SharedCookies`, `InnerTubeClient::set_cookies(&self, Option<CookieJar>)`. Each request reads the current jar.

- [ ] **Step 1: Write the failing unit tests**

Append these tests inside the existing `#[cfg(test)] mod tests { … }` block at the end of `crates/ytm-core/src/auth.rs` (before its closing `}`):

```rust
    #[test]
    fn from_pairs_builds_header() {
        let jar = CookieJar::from_pairs([("SAPISID", "test-sapisid"), ("HSID", "test-cookie")]);
        assert_eq!(
            jar.header().expose(),
            "SAPISID=test-sapisid; HSID=test-cookie"
        );
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn from_pairs_skips_unrepresentable_pairs() {
        let jar = CookieJar::from_pairs([
            ("", "test-cookie"),
            ("A=B", "test-cookie"),
            ("A;B", "test-cookie"),
            ("BAD", "x;y"),
            (" SAPISID ", " test-sapisid "),
        ]);
        assert_eq!(jar.header().expose(), "SAPISID=test-sapisid");
    }

    #[test]
    fn from_pairs_accepts_owned_strings() {
        let pairs = vec![(String::from("SID"), String::from("test-cookie"))];
        assert_eq!(
            CookieJar::from_pairs(pairs).header().expose(),
            "SID=test-cookie"
        );
    }

    #[test]
    fn shared_cookies_clones_share_one_slot() {
        let shared = SharedCookies::default();
        let clone = shared.clone();
        clone.set(Some(CookieJar::from_header("SAPISID=test-sapisid")));
        assert_eq!(
            shared.get().and_then(|jar| jar.sapisid()),
            Some(Secret::new("test-sapisid"))
        );
        shared.set(None);
        assert!(clone.get().is_none());
    }

    #[test]
    fn shared_cookies_debug_hides_values() {
        let shared = SharedCookies::new(Some(CookieJar::from_header("SAPISID=test-sapisid")));
        assert_eq!(format!("{shared:?}"), "SharedCookies(set)");
        assert_eq!(
            format!("{:?}", SharedCookies::default()),
            "SharedCookies(empty)"
        );
    }
```

- [ ] **Step 2: Write the failing client tests**

In `crates/ytm-core/tests/client.rs`, change the import line `use ytm_core::auth::CookieJar;` to:

```rust
use ytm_core::auth::{CookieJar, SharedCookies};
```

Append at the end of the file:

```rust
#[tokio::test]
async fn set_cookies_switches_auth_without_rebuilding_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client.browse("FEmusic_home").await.unwrap();
    client.set_cookies(Some(CookieJar::from_header("SAPISID=test-sapisid")));
    client.browse("FEmusic_home").await.unwrap();
    client.set_cookies(None);
    client.browse("FEmusic_home").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].headers.get("cookie").is_none());
    assert_eq!(
        requests[1].headers.get("cookie").unwrap(),
        "SAPISID=test-sapisid"
    );
    assert!(requests[1].headers.get("authorization").is_some());
    assert!(requests[2].headers.get("cookie").is_none());
}

#[tokio::test]
async fn shared_cookies_update_reaches_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let shared = SharedCookies::default();
    let client =
        InnerTubeClient::with_shared_cookies(config_for(&server), shared.clone()).unwrap();
    shared.set(Some(CookieJar::from_pairs([("SAPISID", "test-sapisid")])));
    client.browse("FEmusic_home").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("cookie").unwrap(),
        "SAPISID=test-sapisid"
    );
    assert!(client.cookies().get().is_some());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ytm-core`
Expected: compile errors — `no function or associated item named 'from_pairs'`, `cannot find type 'SharedCookies'`, `no method named 'set_cookies'`.

- [ ] **Step 4: Implement in `auth.rs`**

Replace the top of `crates/ytm-core/src/auth.rs` (the imports) with:

```rust
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use crate::secret::Secret;

use sha1::{Digest, Sha1};
```

Change the derive on `CookieJar` from `#[derive(Debug, Clone)]` to:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
```

Insert this method in `impl CookieJar`, directly after `from_header`:

```rust
    /// Builds a header from (name, value) pairs, e.g. cookies read from a WebView.
    /// Skips pairs that cannot be represented in a `Cookie` header: an empty name,
    /// a name containing `=` or `;`, or a value containing `;`.
    pub fn from_pairs<I, N, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (N, V)>,
        N: AsRef<str>,
        V: AsRef<str>,
    {
        let header = pairs
            .into_iter()
            .filter_map(|(name, value)| {
                let name = name.as_ref().trim();
                let value = value.as_ref().trim();
                let valid = !name.is_empty() && !name.contains(['=', ';']) && !value.contains(';');
                valid.then(|| format!("{name}={value}"))
            })
            .collect::<Vec<_>>()
            .join("; ");
        Self {
            header: Secret::new(header),
        }
    }
```

Insert after the closing `}` of `impl CookieJar` and before `#[cfg(test)]`:

```rust
/// A cookie jar shared between the client and its owner. Cloning shares the
/// same slot, so `set` on any clone changes the cookies every clone sends.
#[derive(Clone, Default)]
pub struct SharedCookies(Arc<RwLock<Option<CookieJar>>>);

impl SharedCookies {
    pub fn new(jar: Option<CookieJar>) -> Self {
        Self(Arc::new(RwLock::new(jar)))
    }

    pub fn get(&self) -> Option<CookieJar> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, jar: Option<CookieJar>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = jar;
    }
}

impl fmt::Debug for SharedCookies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = if self.get().is_some() { "set" } else { "empty" };
        write!(f, "SharedCookies({state})")
    }
}
```

- [ ] **Step 5: Implement in `client.rs`**

In `crates/ytm-core/src/client.rs`:

1. Change `use crate::auth::{CookieJar, sapisid_hash};` to:

```rust
use crate::auth::{CookieJar, SharedCookies, sapisid_hash};
```

2. Change the struct field `cookies: Option<CookieJar>,` to `cookies: SharedCookies,`.

3. Replace the whole `pub fn new(...) { ... }` with:

```rust
    pub fn new(config: ClientConfig, cookies: Option<CookieJar>) -> Result<Self, YtmError> {
        Self::with_shared_cookies(config, SharedCookies::new(cookies))
    }

    /// Builds a client that reads its cookies from `cookies` on every request,
    /// so sign-in, sign-out, and cookie rotation need no new client.
    pub fn with_shared_cookies(
        config: ClientConfig,
        cookies: SharedCookies,
    ) -> Result<Self, YtmError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            config,
            cookies,
        })
    }

    /// The handle this client reads cookies from.
    pub fn cookies(&self) -> &SharedCookies {
        &self.cookies
    }

    /// Replaces the cookies used by later requests. `None` sends unauthenticated requests.
    pub fn set_cookies(&self, cookies: Option<CookieJar>) {
        self.cookies.set(cookies);
    }
```

4. In `post`, change `if let Some(jar) = &self.cookies {` to:

```rust
        if let Some(jar) = self.cookies.get() {
```

(The rest of the block is unchanged; `jar` is now an owned `CookieJar`.)

- [ ] **Step 6: Run checks**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all pass, including the 5 new unit tests and 2 new client tests.

- [ ] **Step 7: Commit and push**

```bash
git add crates/ytm-core/src/auth.rs crates/ytm-core/src/client.rs crates/ytm-core/tests/client.rs
git commit -m "feat(core): make InnerTubeClient cookies swappable at runtime"
git push origin main
```

---

### Task 2: ytm-core — optional `specta` feature

The app returns `ytm-core` models (`HomePage` and friends) from Tauri commands; `tauri-specta` needs `specta::Type` on them. `specta` is not Tauri, and the feature is off by default, so `ytm-core` stays Tauri-free.

**Files:**
- Modify: `crates/ytm-core/Cargo.toml`
- Modify: `crates/ytm-core/src/models.rs`

**Interfaces:**
- Consumes: Task 1's `ytm-core` (no API change).
- Produces: feature `ytm-core/specta`; with it, `ArtistRef`, `Song`, `BrowseCard`, `ShelfItem`, `HomeSection`, `HomePage` implement `specta::Type`. TypeScript shape (after Task 3's export): fields stay snake_case (`video_id`, `thumbnail_url`, `browse_id`), `ShelfItem` is `{ kind: "song" } & Song | { kind: "album" } & BrowseCard | { kind: "playlist" } & BrowseCard | { kind: "artist" } & BrowseCard`, `Option<T>` is `T | null`.

- [ ] **Step 1: Write the failing test**

Append to the end of `crates/ytm-core/src/models.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ytm-core --features specta --lib`
Expected: error `the package 'ytm-core' does not contain this feature: specta`.

- [ ] **Step 3: Add the dependency and feature**

In `crates/ytm-core/Cargo.toml`, add to `[dependencies]` (keep alphabetical order, after `sha1`):

```toml
specta = { version = "=2.0.0-rc.28", features = ["derive"], optional = true }
```

Add a new section between `[dependencies]` and `[dev-dependencies]`:

```toml
[features]
# Derives `specta::Type` on the models so the Tauri app can export them to TypeScript.
specta = ["dep:specta"]
```

The `=` pin is required: tauri-specta rc.29 depends on exactly `specta =2.0.0-rc.28`.

- [ ] **Step 4: Derive the trait**

In `crates/ytm-core/src/models.rs`, directly below each of the six `#[derive(Debug, Clone, PartialEq, Serialize)]` lines, add:

```rust
#[cfg_attr(feature = "specta", derive(specta::Type))]
```

(For `ShelfItem`, put it between the derive line and the existing `#[serde(tag = "kind", rename_all = "snake_case")]` line.)

- [ ] **Step 5: Run checks with and without the feature**

Run:
```bash
cargo fmt --all
cargo clippy -p ytm-core --all-targets -- -D warnings
cargo clippy -p ytm-core --all-targets --features specta -- -D warnings
cargo test -p ytm-core --features specta
cargo test --workspace
```
Expected: all pass; `models::specta_tests::models_implement_specta_type ... ok` appears only in the `--features specta` run.

- [ ] **Step 6: Commit and push**

```bash
git add crates/ytm-core/Cargo.toml crates/ytm-core/src/models.rs Cargo.lock
git commit -m "feat(core): add optional specta feature for model types"
git push origin main
```

---

### Task 3: App scaffold — Tauri 2 + React + Vite + tauri-specta

Creates a buildable, runnable app with the security baseline, the shared error/status/event types, and the bindings export test. No commands yet (Task 6 adds them).

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `app/package.json`, `app/index.html`, `app/vite.config.ts`, `app/tsconfig.json`, `app/tsconfig.app.json`, `app/tsconfig.node.json`, `app/eslint.config.js`, `app/.gitignore`
- Create: `app/src/main.tsx`, `app/src/styles.css`, `app/src/App.tsx`, `app/src/App.test.tsx`, `app/src/test/setup.ts`
- Create: `app/src-tauri/Cargo.toml`, `app/src-tauri/build.rs`, `app/src-tauri/tauri.conf.json`, `app/src-tauri/capabilities/default.json`, `app/src-tauri/.gitignore`
- Create: `app/src-tauri/src/main.rs`, `app/src-tauri/src/lib.rs`, `app/src-tauri/src/error.rs`, `app/src-tauri/src/events.rs`, `app/src-tauri/src/session/mod.rs`, `app/src-tauri/src/session/status.rs`
- Create (copied): `app/src-tauri/icons/`
- Generated and committed: `app/pnpm-lock.yaml`, `app/src/bindings.ts`, `app/src-tauri/permissions/autogenerated/*.toml`, `Cargo.lock`

**Interfaces:**
- Consumes: `ytm_core::YtmError` (variants `AuthExpired`, `Network(reqwest::Error)`, `Http { status }`, `Parse { endpoint, path }`, `RateLimited`); feature `ytm-core/specta` (Task 2).
- Produces (all in crate `kaset`, lib name `kaset_lib`; modules are `pub` so items unused until later tasks do not trip `dead_code`):
  - `kaset_lib::error::ErrorKind` = `AuthExpired | NotSignedIn | Network | Http | Parse | RateLimited | Internal` (serde snake_case, `specta::Type`).
  - `kaset_lib::error::AppError { pub kind: ErrorKind, pub message: String }` (`Serialize`, `specta::Type`, `thiserror::Error` displaying `message`); `AppError::new(ErrorKind, impl Into<String>)`, `AppError::internal(impl Into<String>)`, `AppError::not_signed_in()`; `From<YtmError>`, `From<tauri::Error>`.
  - `kaset_lib::session::AuthStatus` = `SignedOut | SignedIn | Expired` (serde snake_case → `"signed_out" | "signed_in" | "expired"`; `Serialize, Deserialize, specta::Type, Copy, Eq`).
  - `kaset_lib::events::AuthChanged { pub status: AuthStatus }` with `#[tauri_specta(event_name = "auth://changed")]`; TS: `events.authChanged`.
  - `kaset_lib::specta_builder() -> tauri_specta::Builder<tauri::Wry>`; `kaset_lib::run()`.
  - Test `tests::bindings_are_current` writes `app/src/bindings.ts` (header `// @ts-nocheck`, with `mockCommands` from `.mocks()`).
  - `app/src-tauri/build.rs` already lists the four Task 6 commands `auth_status`, `sign_in`, `sign_out`, `get_home` (tauri-build only generates permission files from the list).
  - pnpm scripts in `app/`: `dev`, `build`, `tauri`, `lint`, `typecheck`, `test`.

- [ ] **Step 1: Add the app to the workspace**

In the root `Cargo.toml`, change `members = ["crates/*"]` to:

```toml
members = ["crates/*", "app/src-tauri"]
```

- [ ] **Step 2: Create the frontend toolchain files**

`app/package.json`:

```json
{
  "name": "kaset-app",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "pnpm@10.29.1",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "lint": "eslint .",
    "typecheck": "tsc -b",
    "test": "vitest run"
  },
  "dependencies": {
    "@tauri-apps/api": "~2.12.5",
    "react": "^19.3.1",
    "react-dom": "^19.3.1"
  },
  "devDependencies": {
    "@eslint/js": "^10.0.1",
    "@tauri-apps/cli": "~2.12.6",
    "@testing-library/jest-dom": "^8.0.0",
    "@testing-library/react": "^16.3.3",
    "@testing-library/user-event": "^14.6.13",
    "@types/react": "^19.3.0",
    "@types/react-dom": "^19.3.0",
    "@vitejs/plugin-react": "^6.3.0",
    "eslint": "^10.15.0",
    "eslint-plugin-react-hooks": "^7.1.1",
    "globals": "^17.14.0",
    "jsdom": "^31.0.1",
    "typescript": "~5.9.3",
    "typescript-eslint": "^8.73.0",
    "vite": "^8.3.8",
    "vitest": "^5.1.1"
  }
}
```

`app/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Kaset</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`app/vite.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and fails if it is taken.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
```

`app/tsconfig.json`:

```json
{
  "files": [],
  "references": [{ "path": "./tsconfig.app.json" }, { "path": "./tsconfig.node.json" }]
}
```

`app/tsconfig.app.json`:

```json
{
  "compilerOptions": {
    "tsBuildInfoFile": "./node_modules/.tmp/tsconfig.app.tsbuildinfo",
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "types": ["@testing-library/jest-dom/vitest"],
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "skipLibCheck": true,
    "noEmit": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true
  },
  "include": ["src"]
}
```

`app/tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "tsBuildInfoFile": "./node_modules/.tmp/tsconfig.node.tsbuildinfo",
    "target": "ES2023",
    "lib": ["ES2023"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true
  },
  "include": ["vite.config.ts", "eslint.config.js"]
}
```

`app/eslint.config.js`:

```js
import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // bindings.ts is generated by tauri-specta.
  { ignores: ["dist", "src-tauri", "src/bindings.ts"] },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: reactHooks.configs.recommended.rules,
  },
);
```

`app/.gitignore`:

```
node_modules
dist
*.tsbuildinfo
```

- [ ] **Step 3: Create the placeholder UI and test setup**

`app/src/main.tsx`:

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

`app/src/styles.css`:

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
```

`app/src/App.tsx` (placeholder; Task 7 replaces it):

```tsx
export function App() {
  return (
    <main className="screen">
      <h1>Kaset</h1>
    </main>
  );
}
```

`app/src/App.test.tsx` (placeholder; Task 7 replaces it):

```tsx
import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import { App } from "./App";

test("renders the app name", () => {
  render(<App />);
  expect(screen.getByRole("heading", { name: "Kaset" })).toBeInTheDocument();
});
```

`app/src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach } from "vitest";

afterEach(async () => {
  cleanup();
  // Let effect cleanups (event unlisten promises) settle while the IPC mock
  // still exists, then remove it.
  await new Promise((resolve) => setTimeout(resolve, 0));
  clearMocks();
});
```

- [ ] **Step 4: Install and check the frontend**

Run:
```bash
cd app && pnpm install && pnpm lint && pnpm typecheck && pnpm test && cd ..
```
Expected: `pnpm-lock.yaml` created; lint and typecheck print nothing; `1 passed`.

- [ ] **Step 5: Create the Tauri crate files**

Copy the placeholder icons:

```bash
mkdir -p app/src-tauri
cp -R spikes/tauri-playback/src-tauri/icons app/src-tauri/icons
```

`app/src-tauri/Cargo.toml`:

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
tokio = { version = "1", features = ["sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
ytm-core = { version = "0.1.0", path = "../../crates/ytm-core", features = ["specta"] }

[dev-dependencies]
tempfile = "3"
```

`app/src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "auth_status",
            "sign_in",
            "sign_out",
            "get_home",
        ]),
    ))
    .expect("failed to run tauri-build");
}
```

`app/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Kaset",
  "version": "0.1.0",
  "identifier": "com.sertacozercan.kaset.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [
      {
        "label": "main",
        "title": "Kaset",
        "width": 1100,
        "height": 720,
        "minWidth": 800,
        "minHeight": 500
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://i.ytimg.com https://*.ggpht.com https://*.googleusercontent.com; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'",
      "devCsp": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://i.ytimg.com https://*.ggpht.com https://*.googleusercontent.com; connect-src 'self' ipc: http://ipc.localhost ws://localhost:1420; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

`app/src-tauri/capabilities/default.json`:

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
    "allow-get-home"
  ]
}
```

`app/src-tauri/.gitignore`:

```
# Generated by Tauri: capability schemas
/gen/schemas
```

`app/src-tauri/src/main.rs`:

```rust
// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kaset_lib::run();
}
```

- [ ] **Step 6: Write the Rust tests first (error mapping + bindings export)**

`app/src-tauri/src/error.rs` — create it with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ytm_errors_to_kinds() {
        assert_eq!(
            AppError::from(YtmError::AuthExpired).kind,
            ErrorKind::AuthExpired
        );
        assert_eq!(
            AppError::from(YtmError::Http { status: 500 }).kind,
            ErrorKind::Http
        );
        assert_eq!(
            AppError::from(YtmError::RateLimited).kind,
            ErrorKind::RateLimited
        );
        let parse = AppError::from(YtmError::Parse {
            endpoint: "browse".into(),
            path: "contents".into(),
        });
        assert_eq!(parse.kind, ErrorKind::Parse);
        assert_eq!(parse.message, "could not parse browse response at contents");
    }

    #[test]
    fn serializes_as_kind_and_message() {
        let json = serde_json::to_value(AppError::not_signed_in()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "not_signed_in", "message": "Sign in to continue." })
        );
    }
}
```

`app/src-tauri/src/lib.rs` — create with the module list and the export test only:

```rust
pub mod error;

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

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -p kaset`
Expected: compile errors — `cannot find type 'AppError'`, `cannot find function 'specta_builder'`. (The first build of Tauri takes a few minutes. If `tauri-build` complains about a missing `frontendDist`, check that `devUrl` is set: debug builds without the `custom-protocol` feature use `devUrl` and embed no assets.)

- [ ] **Step 8: Implement error, status, event, and builder**

Prepend to `app/src-tauri/src/error.rs` (above the test module):

```rust
use serde::Serialize;
use ytm_core::YtmError;

/// What went wrong, for the UI to branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    AuthExpired,
    NotSignedIn,
    Network,
    Http,
    Parse,
    RateLimited,
    Internal,
}

/// Error returned by every command. Serializes as `{ kind, message }`.
/// `message` is for display and never contains cookies or headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    pub fn not_signed_in() -> Self {
        Self::new(ErrorKind::NotSignedIn, "Sign in to continue.")
    }
}

impl From<YtmError> for AppError {
    fn from(error: YtmError) -> Self {
        let kind = match &error {
            YtmError::AuthExpired => ErrorKind::AuthExpired,
            YtmError::Network(_) => ErrorKind::Network,
            YtmError::Http { .. } => ErrorKind::Http,
            YtmError::Parse { .. } => ErrorKind::Parse,
            YtmError::RateLimited => ErrorKind::RateLimited,
        };
        Self::new(kind, error.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(error: tauri::Error) -> Self {
        Self::internal(error.to_string())
    }
}
```

`app/src-tauri/src/session/status.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Sign-in state shown to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    /// No session. The UI shows the sign-in screen.
    SignedOut,
    /// Cookies with SAPISID are loaded and sent with API requests.
    SignedIn,
    /// The API answered 401/403. The UI asks the user to sign in again.
    Expired,
}
```

`app/src-tauri/src/session/mod.rs` (Task 5 replaces this file):

```rust
mod status;

pub use status::AuthStatus;
```

`app/src-tauri/src/events.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::session::AuthStatus;

/// Emitted on `auth://changed` whenever the sign-in state changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "auth://changed")]
pub struct AuthChanged {
    pub status: AuthStatus,
}
```

Replace the top of `app/src-tauri/src/lib.rs` (everything above `#[cfg(test)]`) with:

```rust
pub mod error;
pub mod events;
pub mod session;

use tauri_specta::{Builder, collect_commands, collect_events};

/// Commands and events exposed to the UI. `bindings_are_current` exports
/// them to `app/src/bindings.ts`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![])
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Kaset");
}
```

- [ ] **Step 9: Run Rust checks and regenerate bindings**

Run:
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all pass; `kaset` runs 3 tests (`maps_ytm_errors_to_kinds`, `serializes_as_kind_and_message`, `bindings_are_current`). `app/src/bindings.ts` now exists and contains `authChanged: makeEvent<AuthChanged, AuthChanged>("auth://changed")` and `export type AuthStatus = … "signed_out" | … "signed_in" | … "expired";`. `app/src-tauri/permissions/autogenerated/` contains `auth_status.toml`, `get_home.toml`, `sign_in.toml`, `sign_out.toml`.

- [ ] **Step 10: Re-run frontend checks with the generated bindings**

Run: `cd app && pnpm lint && pnpm typecheck && pnpm test && cd ..`
Expected: pass (bindings.ts is ignored by ESLint and has `// @ts-nocheck`).

- [ ] **Step 11: Smoke-build the bundle**

Run: `cd app && pnpm tauri build --bundles app && cd ..` (on Windows: `--bundles nsis`)
Expected: ends with a bundle path under the workspace target dir, e.g. `target/release/bundle/macos/Kaset.app`. Do not commit anything under `target/` or `app/dist/`.

- [ ] **Step 12: Commit and push**

```bash
git add Cargo.toml Cargo.lock app/package.json app/pnpm-lock.yaml app/index.html app/vite.config.ts \
  app/tsconfig.json app/tsconfig.app.json app/tsconfig.node.json app/eslint.config.js app/.gitignore \
  app/src/main.tsx app/src/styles.css app/src/App.tsx app/src/App.test.tsx app/src/test/setup.ts app/src/bindings.ts \
  app/src-tauri/Cargo.toml app/src-tauri/build.rs app/src-tauri/tauri.conf.json app/src-tauri/.gitignore \
  app/src-tauri/capabilities/default.json app/src-tauri/permissions app/src-tauri/icons \
  app/src-tauri/src/main.rs app/src-tauri/src/lib.rs app/src-tauri/src/error.rs app/src-tauri/src/events.rs \
  app/src-tauri/src/session/mod.rs app/src-tauri/src/session/status.rs
git status --short   # must show nothing else staged; app/node_modules, app/dist, target/ stay untracked/ignored
git commit -m "feat(app): scaffold Tauri 2 app with React, Vite and tauri-specta"
git push origin main
```

---

### Task 4: Session store — keyring (chunked) and debug file store

**Files:**
- Create: `app/src-tauri/src/session/store.rs`
- Modify: `app/src-tauri/src/session/mod.rs`

**Interfaces:**
- Consumes: `ytm_core::secret::Secret` (`new`, `expose`, `Clone`, `PartialEq`); `keyring` 4.2 v1 API.
- Produces (module `kaset_lib::session::store`):
  - consts `KEYRING_SERVICE = "com.sertacozercan.kaset.desktop"`, `STORE_ENV = "KASET_SESSION_STORE"`, `DEV_FILE_NAME = "dev-session.txt"`, `CHUNK_SIZE = 2000`.
  - `enum StoreError { Keyring(&'static str), Io(io::Error), Corrupt(&'static str) }` (`thiserror`, never contains secret bytes).
  - `trait SessionStore: Send + Sync { fn load(&self) -> Result<Option<Secret>, StoreError>; fn save(&self, header: &Secret) -> Result<(), StoreError>; fn clear(&self) -> Result<(), StoreError>; }`
  - `trait EntryBackend: Send + Sync { get(&self, &str) -> Result<Option<Vec<u8>>, StoreError>; set(&self, &str, &[u8]) -> Result<(), StoreError>; delete(&self, &str) -> Result<(), StoreError> /* Ok if missing */ }`
  - `struct ChunkedStore<B: EntryBackend>` (`new(B)`), implements `SessionStore`; `struct KeyringBackend` (`new(&'static str)`), implements `EntryBackend`; `struct FileStore` (`new(PathBuf)`), implements `SessionStore`.
  - `enum StoreKind { Keyring, File }`; `fn store_kind(env_value: Option<&str>, debug_build: bool) -> StoreKind`; `fn store_for(kind: StoreKind, app_data_dir: &Path) -> Box<dyn SessionStore>`.
  - `#[cfg(test)] struct MemoryStore` (`Default`, `MemoryStore::failing()` whose `save` errors) — used by Task 5's tests.

- [ ] **Step 1: Declare the module**

Replace `app/src-tauri/src/session/mod.rs` with:

```rust
mod status;
pub mod store;

pub use status::AuthStatus;
```

- [ ] **Step 2: Write the store with its tests**

Create `app/src-tauri/src/session/store.rs`. Write the test module first (bottom of the file), then the implementation above it; the complete file is:

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

const COUNT_ENTRY: &str = "session.count";

fn chunk_entry(index: usize) -> String {
    format!("session.{index}")
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

/// Splits the header across `session.0..N` entries plus a `session.count` entry.
pub struct ChunkedStore<B: EntryBackend> {
    backend: B,
}

impl<B: EntryBackend> ChunkedStore<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    fn count(&self) -> Result<Option<usize>, StoreError> {
        let Some(raw) = self.backend.get(COUNT_ENTRY)? else {
            return Ok(None);
        };
        let text = std::str::from_utf8(&raw).map_err(|_| StoreError::Corrupt("count"))?;
        text.parse()
            .map(Some)
            .map_err(|_| StoreError::Corrupt("count"))
    }
}

impl<B: EntryBackend> SessionStore for ChunkedStore<B> {
    fn load(&self) -> Result<Option<Secret>, StoreError> {
        let Some(count) = self.count()? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        for index in 0..count {
            let chunk = self
                .backend
                .get(&chunk_entry(index))?
                .ok_or(StoreError::Corrupt("missing chunk"))?;
            bytes.extend_from_slice(&chunk);
        }
        let header = String::from_utf8(bytes).map_err(|_| StoreError::Corrupt("encoding"))?;
        Ok(Some(Secret::new(header)))
    }

    fn save(&self, header: &Secret) -> Result<(), StoreError> {
        let old_count = self.count().unwrap_or(None).unwrap_or(0);
        let bytes = header.expose().as_bytes();
        let chunks: Vec<&[u8]> = bytes.chunks(CHUNK_SIZE).collect();
        for (index, chunk) in chunks.iter().enumerate() {
            self.backend.set(&chunk_entry(index), chunk)?;
        }
        // The count is written last: a save interrupted before this point
        // leaves the previous count in place.
        self.backend
            .set(COUNT_ENTRY, chunks.len().to_string().as_bytes())?;
        for index in chunks.len()..old_count {
            self.backend.delete(&chunk_entry(index))?;
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        let count = self.count().unwrap_or(None).unwrap_or(0);
        self.backend.delete(COUNT_ENTRY)?;
        for index in 0..count {
            self.backend.delete(&chunk_entry(index))?;
        }
        Ok(())
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
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct MapBackend(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);

    impl MapBackend {
        fn names(&self) -> Vec<String> {
            self.0.lock().unwrap().keys().cloned().collect()
        }
    }

    impl EntryBackend for MapBackend {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self.0.lock().unwrap().get(name).cloned())
        }
        fn set(&self, name: &str, value: &[u8]) -> Result<(), StoreError> {
            self.0.lock().unwrap().insert(name.into(), value.to_vec());
            Ok(())
        }
        fn delete(&self, name: &str) -> Result<(), StoreError> {
            self.0.lock().unwrap().remove(name);
            Ok(())
        }
    }

    fn long_header(len: usize) -> Secret {
        let value = "x".repeat(len - "SAPISID=".len());
        Secret::new(format!("SAPISID={value}"))
    }

    #[test]
    fn chunked_store_round_trips_a_short_header() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&Secret::new("SAPISID=test-sapisid")).unwrap();
        assert_eq!(
            store.load().unwrap(),
            Some(Secret::new("SAPISID=test-sapisid"))
        );
        assert_eq!(backend.names(), ["session.0", "session.count"]);
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
            ["session.0", "session.1", "session.2", "session.count"]
        );
    }

    #[test]
    fn chunked_store_removes_stale_chunks_when_header_shrinks() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        store.save(&Secret::new("SAPISID=test-sapisid")).unwrap();
        assert_eq!(backend.names(), ["session.0", "session.count"]);
        assert_eq!(
            store.load().unwrap(),
            Some(Secret::new("SAPISID=test-sapisid"))
        );
    }

    #[test]
    fn chunked_store_load_is_none_when_empty() {
        let store = ChunkedStore::new(MapBackend::default());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn chunked_store_rejects_missing_chunks() {
        let backend = MapBackend::default();
        backend.set("session.count", b"2").unwrap();
        backend.set("session.0", b"SAPISID=").unwrap();
        let store = ChunkedStore::new(backend);
        assert!(matches!(
            store.load(),
            Err(StoreError::Corrupt("missing chunk"))
        ));
    }

    #[test]
    fn chunked_store_clear_removes_everything() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE + 1)).unwrap();
        store.clear().unwrap();
        assert!(backend.names().is_empty());
        assert_eq!(store.load().unwrap(), None);
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

TDD order inside this step: paste the `#[cfg(test)] mod tests` block and the `MemoryStore`/`MapBackend` helpers first, run `cargo test -p kaset session::store` and confirm it fails to compile (`cannot find type 'ChunkedStore'`), then add the implementation above.

No test touches the real Keychain or Credential Manager: `KeyringBackend` is exercised only by the human check at the end of the plan.

- [ ] **Step 3: Run checks**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p kaset`
Expected: all pass; 10 `session::store::tests::*` tests (9 on Windows, where `file_store_is_owner_only` is compiled out).

- [ ] **Step 4: Commit and push**

```bash
git add app/src-tauri/src/session/mod.rs app/src-tauri/src/session/store.rs
git commit -m "feat(app): persist the session in the OS secure store"
git push origin main
```

---

### Task 5: Session state machine and WebView cookie selection

**Files:**
- Create: `app/src-tauri/src/session/cookies.rs`
- Modify (replace): `app/src-tauri/src/session/mod.rs`

**Interfaces:**
- Consumes: Task 1 `CookieJar::from_pairs`, `CookieJar: PartialEq`, `SharedCookies`; Task 4 `SessionStore`, `StoreError`, `#[cfg(test)] MemoryStore` (`Default`, `failing()`); Task 3 `AuthStatus`; `tauri::webview::Cookie` (re-export of the `cookie` crate: `Cookie::build((name, value)).domain(d).build()`, `name()`, `value()`, `domain() -> Option<&str>`).
- Produces:
  - `kaset_lib::session::cookies::{is_youtube_domain(&str) -> bool, is_session_domain(&str) -> bool, youtube_cookie_jar(&[Cookie<'_>]) -> CookieJar, session_cookies(Vec<Cookie<'static>>) -> Vec<Cookie<'static>>}`.
  - `kaset_lib::session::SessionError { NoSapisid, Store(StoreError) }`.
  - `kaset_lib::session::Session`: `new(Box<dyn SessionStore>) -> Self`, `cookies(&self) -> &SharedCookies`, `status(&self) -> AuthStatus`, `restore(&self) -> AuthStatus`, `sign_in(&self, CookieJar) -> Result<Option<AuthStatus>, SessionError>`, `refresh(&self, CookieJar) -> bool`, `mark_expired(&self) -> Option<AuthStatus>`, `sign_out(&self) -> Result<Option<AuthStatus>, SessionError>`. `Some(status)` means "changed; emit `auth://changed`".

- [ ] **Step 1: Write the cookie selection module with tests**

Create `app/src-tauri/src/session/cookies.rs` (tests first, then the functions above them; complete file):

```rust
//! Selecting cookies read from the WebView with `Webview::cookies()`.
//!
//! Never use `Webview::cookies_for_url`: wry's macOS implementation compares the
//! cookie domain to the URL host exactly, so `.youtube.com` cookies never match
//! `music.youtube.com` (see docs/superpowers/spikes/2026-11-22-tauri-playback.md).

use tauri::webview::Cookie;
use ytm_core::auth::CookieJar;

fn normalize(domain: &str) -> String {
    domain.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn is_under(domain: &str, root: &str) -> bool {
    let domain = normalize(domain);
    domain == root || domain.ends_with(&format!(".{root}"))
}

/// `youtube.com` or any subdomain, with or without a leading dot.
pub fn is_youtube_domain(domain: &str) -> bool {
    is_under(domain, "youtube.com")
}

/// Domains whose cookies make up a Google sign-in: youtube.com and google.com.
/// Sign-out deletes every cookie under these.
pub fn is_session_domain(domain: &str) -> bool {
    is_under(domain, "youtube.com") || is_under(domain, "google.com")
}

/// Builds the API cookie jar from the youtube.com cookies in `cookies`.
pub fn youtube_cookie_jar(cookies: &[Cookie<'_>]) -> CookieJar {
    CookieJar::from_pairs(
        cookies
            .iter()
            .filter(|cookie| cookie.domain().is_some_and(is_youtube_domain))
            .map(|cookie| (cookie.name(), cookie.value())),
    )
}

/// The cookies sign-out must delete.
pub fn session_cookies(cookies: Vec<Cookie<'static>>) -> Vec<Cookie<'static>> {
    cookies
        .into_iter()
        .filter(|cookie| cookie.domain().is_some_and(is_session_domain))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ytm_core::secret::Secret;

    fn cookie(name: &'static str, value: &'static str, domain: &'static str) -> Cookie<'static> {
        Cookie::build((name, value)).domain(domain).build()
    }

    #[test]
    fn youtube_domain_matching() {
        assert!(is_youtube_domain(".youtube.com"));
        assert!(is_youtube_domain("youtube.com"));
        assert!(is_youtube_domain("music.youtube.com"));
        assert!(is_youtube_domain(".Music.YouTube.com"));
        assert!(!is_youtube_domain("notyoutube.com"));
        assert!(!is_youtube_domain("youtube.com.evil.example"));
        assert!(!is_youtube_domain(".google.com"));
        assert!(!is_youtube_domain(""));
    }

    #[test]
    fn session_domain_matching() {
        assert!(is_session_domain(".google.com"));
        assert!(is_session_domain("accounts.google.com"));
        assert!(is_session_domain(".youtube.com"));
        assert!(!is_session_domain("google.com.evil.example"));
        assert!(!is_session_domain("example.com"));
    }

    #[test]
    fn jar_keeps_only_youtube_cookies() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("HSID", "test-cookie", "music.youtube.com"),
            cookie("SID", "test-google-cookie", ".google.com"),
            cookie("OTHER", "test-cookie", "example.com"),
            Cookie::new("NODOMAIN", "test-cookie"),
        ];
        let jar = youtube_cookie_jar(&cookies);
        assert_eq!(
            jar.header().expose(),
            "SAPISID=test-sapisid; HSID=test-cookie"
        );
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn jar_prefers_secure_sapisid_from_webview_cookies() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("__Secure-3PAPISID", "test-secure-sapisid", ".youtube.com"),
        ];
        assert_eq!(
            youtube_cookie_jar(&cookies).sapisid(),
            Some(Secret::new("test-secure-sapisid"))
        );
    }

    #[test]
    fn session_cookies_cover_google_and_youtube() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("SID", "test-google-cookie", ".google.com"),
            cookie("OTHER", "test-cookie", "example.com"),
        ];
        let names: Vec<_> = session_cookies(cookies)
            .iter()
            .map(|cookie| cookie.name().to_owned())
            .collect();
        assert_eq!(names, ["SAPISID", "SID"]);
    }
}
```

- [ ] **Step 2: Write the session tests (failing)**

Replace `app/src-tauri/src/session/mod.rs` with the module declarations and the tests only:

```rust
//! Sign-in state and cookie ownership. No Tauri types: the window and event
//! glue lives in `login.rs` and `commands.rs`, so this is testable alone.

pub mod cookies;
mod status;
pub mod store;

pub use status::AuthStatus;

#[cfg(test)]
mod tests {
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
        assert_eq!(session.mark_expired(), None);
        session.sign_in(jar("test-sapisid")).unwrap();
        assert_eq!(session.mark_expired(), Some(AuthStatus::Expired));
        assert_eq!(session.mark_expired(), None);
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
            Some(Secret::new("SAPISID=test-sapisid-rotated; HSID=test-cookie"))
        );
        assert!(!session.refresh(CookieJar::from_pairs([("HSID", "test-cookie")])));
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
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p kaset session::`
Expected: compile error `cannot find type 'Session' in this scope` (the `cookies` tests would pass on their own).

- [ ] **Step 4: Implement `Session`**

Insert between `pub use status::AuthStatus;` and `#[cfg(test)]` in `app/src-tauri/src/session/mod.rs`:

```rust
use std::sync::{Mutex, PoisonError};

use store::{SessionStore, StoreError};
use ytm_core::auth::{CookieJar, SharedCookies};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("the sign-in cookies have no SAPISID")]
    NoSapisid,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Owns the cookies the API client sends and the persisted copy.
///
/// Every mutating method returns `Some(new_status)` when the status changed,
/// so the caller knows to emit `auth://changed`.
pub struct Session {
    cookies: SharedCookies,
    status: Mutex<AuthStatus>,
    store: Box<dyn SessionStore>,
}

impl Session {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            cookies: SharedCookies::default(),
            status: Mutex::new(AuthStatus::SignedOut),
            store,
        }
    }

    /// The handle to give `InnerTubeClient::with_shared_cookies`.
    pub fn cookies(&self) -> &SharedCookies {
        &self.cookies
    }

    pub fn status(&self) -> AuthStatus {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_status(&self, next: AuthStatus) -> Option<AuthStatus> {
        let mut status = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        (*status != next).then(|| {
            *status = next;
            next
        })
    }

    /// Loads the stored session at launch. A store error or a stored header
    /// without SAPISID leaves the user signed out.
    pub fn restore(&self) -> AuthStatus {
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
                self.set_status(AuthStatus::SignedIn);
            }
            None => {
                self.cookies.set(None);
                self.set_status(AuthStatus::SignedOut);
            }
        }
        self.status()
    }

    /// Completes a sign-in with cookies read from the login WebView.
    /// A failed save keeps the in-memory session and is logged; the user stays
    /// signed in until the app quits.
    pub fn sign_in(&self, jar: CookieJar) -> Result<Option<AuthStatus>, SessionError> {
        if jar.sapisid().is_none() {
            return Err(SessionError::NoSapisid);
        }
        if let Err(error) = self.store.save(jar.header()) {
            tracing::warn!(%error, "signed in, but the session could not be saved");
        }
        self.cookies.set(Some(jar));
        Ok(self.set_status(AuthStatus::SignedIn))
    }

    /// Picks up rotated cookies from the WebView. Ignored unless signed in,
    /// so it can never sign a user back in after sign-out.
    pub fn refresh(&self, jar: CookieJar) -> bool {
        if self.status() != AuthStatus::SignedIn || jar.sapisid().is_none() {
            return false;
        }
        if self.cookies.get().as_ref() == Some(&jar) {
            return false;
        }
        if let Err(error) = self.store.save(jar.header()) {
            tracing::warn!(%error, "could not save refreshed cookies");
        }
        self.cookies.set(Some(jar));
        true
    }

    /// The API answered 401/403. Keeps the cookies (a new sign-in replaces
    /// them) and asks the UI to prompt for sign-in.
    pub fn mark_expired(&self) -> Option<AuthStatus> {
        if self.status() != AuthStatus::SignedIn {
            return None;
        }
        self.set_status(AuthStatus::Expired)
    }

    /// Forgets the session in memory and in the store. The in-memory state is
    /// cleared even when the store fails, and the error is returned.
    pub fn sign_out(&self) -> Result<Option<AuthStatus>, SessionError> {
        self.cookies.set(None);
        let changed = self.set_status(AuthStatus::SignedOut);
        self.store.clear()?;
        Ok(changed)
    }
}
```

Leave the test module's imports as written in Step 2 (`use store::{MemoryStore, SessionStore};` and `use ytm_core::auth::CookieJar;`); the explicit imports shadow the glob without a warning.

- [ ] **Step 5: Run checks**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p kaset`
Expected: all pass — 11 `session::tests::*` and 5 `session::cookies::tests::*` tests plus the earlier ones.

- [ ] **Step 6: Commit and push**

```bash
git add app/src-tauri/src/session/mod.rs app/src-tauri/src/session/cookies.rs
git commit -m "feat(app): add session state machine and WebView cookie selection"
git push origin main
```

---

### Task 6: Login window, commands, and launch restore

Wires the session to Tauri: the login window (Safari UA on macOS, navigation allowlist, completion on music.youtube.com), the four commands, `auth://changed`, restore at launch, the launch-time cookie refresh, and `AuthExpired` → `Expired`.

**Files:**
- Create: `app/src-tauri/src/login.rs`, `app/src-tauri/src/state.rs`, `app/src-tauri/src/commands.rs`
- Modify: `app/src-tauri/src/lib.rs`
- Regenerated: `app/src/bindings.ts`

**Interfaces:**
- Consumes: Task 1 `InnerTubeClient::with_shared_cookies`; `ytm_core::api_key::{API_KEY_ENV, fetch_api_key(web_url: &str) -> Result<String, YtmError>}`; `ytm_core::client::{ClientConfig::music(api_key), MUSIC_ORIGIN}`; `ytm_core::parsers::home::parse_home(&Value) -> Result<HomePage, YtmError>`; `ytm_core::models::HomePage`; Task 3 `AppError`, `ErrorKind`, `AuthChanged`, `specta_builder`; Task 4 `STORE_ENV`, `store_kind`, `store_for`; Task 5 `Session`, `youtube_cookie_jar`, `session_cookies`.
- Produces:
  - `kaset_lib::login::{LOGIN_LABEL = "login", LOGIN_URL, is_allowed_login_navigation(&Url) -> bool, is_sign_in_landing(&Url) -> bool, open_login_window(&AppHandle) -> Result<(), AppError>, emit_auth_changed(&AppHandle, AuthStatus)}`; macOS-only `SAFARI_USER_AGENT`.
  - `kaset_lib::state::AppState { pub session: Session, .. }`: `AppState::new(Session)`, `async fn client(&self) -> Result<Arc<InnerTubeClient>, AppError>` (built once, shares `session.cookies()`, honours `KASET_YTMUSIC_API_KEY`).
  - Commands (TS names in parentheses): `auth_status` (`commands.authStatus(): Promise<AuthStatus>`), `sign_in` (`commands.signIn(): Promise<null>`), `sign_out` (`commands.signOut(): Promise<null>`), `get_home` (`commands.getHome(): Promise<HomePage>`). All reject with `AppError`. `get_home` rejects with `kind: "not_signed_in"` unless signed in, and on `auth_expired` also moves the session to `Expired` and emits `auth://changed`.

- [ ] **Step 1: Write the login policy tests (failing)**

Create `app/src-tauri/src/login.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        is_allowed_login_navigation(&url.parse().unwrap())
    }

    #[test]
    fn allows_google_sign_in_hosts() {
        assert!(allowed(LOGIN_URL));
        assert!(allowed("https://accounts.youtube.com/accounts/SetSID"));
        assert!(allowed("https://www.youtube.com/signin?action_handle_signin=true"));
        assert!(allowed("https://music.youtube.com/"));
        assert!(allowed("https://consent.youtube.com/m"));
        assert!(allowed("https://www.google.com/recaptcha/api2/anchor"));
        assert!(allowed("https://ssl.gstatic.com/accounts/static.js"));
        assert!(allowed("https://accounts.google.co.kr/accounts/SetSID"));
        assert!(allowed("https://www.google.com.au/"));
        assert!(allowed("https://accounts.google.de/"));
    }

    #[test]
    fn allows_frame_schemes_without_a_host() {
        assert!(allowed("about:blank"));
        assert!(allowed("about:srcdoc"));
        assert!(allowed("blob:https://accounts.google.com/0f3c"));
        assert!(allowed("data:text/html,hi"));
    }

    #[test]
    fn blocks_everything_else() {
        assert!(!allowed("http://accounts.google.com/"));
        assert!(!allowed("https://example.com/"));
        assert!(!allowed("https://google.com.evil.example/"));
        assert!(!allowed("https://youtube.com.evil.example/"));
        assert!(!allowed("https://notgoogle.com/"));
        assert!(!allowed("https://google.evil/"));
        assert!(!allowed("https://google.co.evil/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed("tauri://localhost/"));
    }

    #[test]
    fn sign_in_lands_on_music_youtube_com() {
        assert!(is_sign_in_landing(
            &"https://music.youtube.com/".parse().unwrap()
        ));
        assert!(is_sign_in_landing(
            &"https://music.youtube.com/library".parse().unwrap()
        ));
        assert!(!is_sign_in_landing(
            &"https://www.youtube.com/".parse().unwrap()
        ));
        assert!(!is_sign_in_landing(&LOGIN_URL.parse().unwrap()));
        assert!(!is_sign_in_landing(
            &"http://music.youtube.com/".parse().unwrap()
        ));
    }
}
```

Add `pub mod login;` to the module list at the top of `app/src-tauri/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p kaset login::`
Expected: compile errors — `cannot find function 'is_allowed_login_navigation'`, `cannot find value 'LOGIN_URL'`.

- [ ] **Step 3: Implement `state.rs`**

Create `app/src-tauri/src/state.rs`:

```rust
use std::sync::Arc;

use tokio::sync::OnceCell;
use ytm_core::api_key::{API_KEY_ENV, fetch_api_key};
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};

use crate::error::AppError;
use crate::session::Session;

/// Managed Tauri state.
pub struct AppState {
    pub session: Session,
    client: OnceCell<Arc<InnerTubeClient>>,
}

impl AppState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
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
```

- [ ] **Step 4: Implement `login.rs`**

Prepend to `app/src-tauri/src/login.rs` (above the test module):

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
use crate::session::AuthStatus;
use crate::session::cookies::youtube_cookie_jar;
use crate::state::AppState;

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

/// Country Google domains such as `accounts.google.co.kr` or `google.com.au`,
/// which the sign-in flow can visit to set cookies.
fn is_country_google(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let Some(google) = labels.iter().rposition(|label| *label == "google") else {
        return false;
    };
    let is_country_code = |tld: &str| tld.len() == 2 && tld.chars().all(|c| c.is_ascii_lowercase());
    match &labels[google + 1..] {
        [tld] => is_country_code(tld),
        [second, tld] => matches!(*second, "co" | "com") && is_country_code(tld),
        _ => false,
    }
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
                tauri::async_runtime::spawn(async move { complete_sign_in(app).await });
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(SAFARI_USER_AGENT);
    builder.build()?;
    Ok(())
}

/// Reads the login window's cookies and signs in when SAPISID is present.
/// Runs on the async runtime: `cookies()` deadlocks on Windows when called
/// from the main thread or a synchronous handler.
async fn complete_sign_in(app: AppHandle) {
    let Some(window) = app.get_webview_window(LOGIN_LABEL) else {
        return;
    };
    let cookies = match window.cookies() {
        Ok(cookies) => cookies,
        Err(error) => {
            tracing::warn!(%error, "could not read login cookies");
            return;
        }
    };
    let jar = youtube_cookie_jar(&cookies);
    if jar.sapisid().is_none() {
        tracing::debug!("music.youtube.com loaded without SAPISID; waiting for sign-in");
        return;
    }
    let state = app.state::<AppState>();
    match state.session.sign_in(jar) {
        Ok(changed) => {
            if let Some(status) = changed {
                emit_auth_changed(&app, status);
            }
            close_login_window(&window);
        }
        Err(error) => tracing::warn!(%error, "sign-in failed"),
    }
}

fn close_login_window(window: &WebviewWindow) {
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

- [ ] **Step 5: Implement `commands.rs`**

Create `app/src-tauri/src/commands.rs`:

```rust
use tauri::{AppHandle, Manager, State};
use ytm_core::models::HomePage;
use ytm_core::parsers::home::parse_home;

use crate::error::{AppError, ErrorKind};
use crate::login::{LOGIN_LABEL, emit_auth_changed, open_login_window};
use crate::session::AuthStatus;
use crate::session::cookies::session_cookies;
use crate::state::AppState;

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
pub async fn sign_out(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        let _ = window.close();
    }
    let result = state.session.sign_out();
    emit_auth_changed(&app, AuthStatus::SignedOut);
    clear_webview_cookies(&app);
    result
        .map(|_| ())
        .map_err(|error| AppError::internal(error.to_string()))
}

/// Deletes youtube.com and google.com cookies from the shared WebView store,
/// re-reading up to three times like the Swift app's cookie clearer.
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

/// Fetches and parses the signed-in home feed.
#[tauri::command]
#[specta::specta]
pub async fn get_home(app: AppHandle, state: State<'_, AppState>) -> Result<HomePage, AppError> {
    if state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    let result = async {
        let client = state.client().await?;
        let response = client.browse("FEmusic_home").await?;
        Ok::<_, AppError>(parse_home(&response)?)
    }
    .await;
    result.map_err(|error| on_error(&app, &state, error))
}

/// Moves the session to `Expired` on an auth error and passes the error on.
fn on_error(app: &AppHandle, state: &AppState, error: AppError) -> AppError {
    if error.kind == ErrorKind::AuthExpired
        && let Some(status) = state.session.mark_expired()
    {
        emit_auth_changed(app, status);
    }
    error
}
```

- [ ] **Step 6: Wire `lib.rs`**

Replace everything in `app/src-tauri/src/lib.rs` above `#[cfg(test)]` with:

```rust
pub mod commands;
pub mod error;
pub mod events;
pub mod login;
pub mod session;
pub mod state;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::session::Session;
use crate::session::cookies::youtube_cookie_jar;
use crate::session::store::{STORE_ENV, store_for, store_kind};
use crate::state::AppState;

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

/// Picks up cookies the WebView rotated since the session was saved.
/// `Session::refresh` ignores this unless the user is signed in.
fn refresh_cookies_from_webview(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        match window.cookies() {
            Ok(cookies) => {
                let state = app.state::<AppState>();
                if state.session.refresh(youtube_cookie_jar(&cookies)) {
                    tracing::info!("refreshed session cookies from the WebView");
                }
            }
            Err(error) => tracing::warn!(%error, "could not read WebView cookies at launch"),
        }
    });
}
```

(The `#[cfg(test)] mod tests { … bindings_are_current … }` block at the bottom stays unchanged.)

- [ ] **Step 7: Run checks and regenerate bindings**

Run:
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all pass, including 4 `login::tests::*`. `app/src/bindings.ts` now contains:

```ts
export const commands = {
	…
	authStatus: (options?: InvokeOptions) => __TAURI_INVOKE<AuthStatus>("auth_status", undefined, options),
	…
	signIn: (options?: InvokeOptions) => __TAURI_INVOKE<null>("sign_in", undefined, options),
	…
	signOut: (options?: InvokeOptions) => __TAURI_INVOKE<null>("sign_out", undefined, options),
	…
	getHome: (options?: InvokeOptions) => __TAURI_INVOKE<HomePage>("get_home", undefined, options),
};
```

and the types `AppError`, `ErrorKind`, `HomePage`, `HomeSection`, `ShelfItem`, `Song`, `BrowseCard`, `ArtistRef`, plus `mockCommands` / `MockCommandHandlers` with `authStatus`, `signIn`, `signOut`, `getHome`.

- [ ] **Step 8: Re-run frontend checks**

Run: `cd app && pnpm lint && pnpm typecheck && pnpm test && cd ..`
Expected: pass (the placeholder UI does not use the commands yet).

- [ ] **Step 9: Commit and push**

```bash
git add app/src-tauri/src/lib.rs app/src-tauri/src/login.rs app/src-tauri/src/state.rs \
  app/src-tauri/src/commands.rs app/src/bindings.ts
git commit -m "feat(app): sign in through a Google login window and expose auth commands"
git push origin main
```

---

### Task 7: Sign-in and home screens

**Files:**
- Create: `app/src/errors.ts`, `app/src/auth/useAuthStatus.ts`, `app/src/auth/SignInScreen.tsx`, `app/src/auth/SignInScreen.test.tsx`, `app/src/home/HomeScreen.tsx`
- Modify (replace): `app/src/App.tsx`, `app/src/App.test.tsx`

**Interfaces:**
- Consumes (from `app/src/bindings.ts`, Task 6): `commands.authStatus(): Promise<AuthStatus>`, `commands.signIn(): Promise<null>`, `commands.signOut(): Promise<null>`, `commands.getHome(): Promise<HomePage>`; `events.authChanged.listen(cb) / .emit(payload)`; types `AuthStatus = "signed_out" | "signed_in" | "expired"`, `AppError = { kind: ErrorKind; message: string }`, `HomePage = { sections: HomeSection[]; continuation: string | null }`, `HomeSection = { title: string; items: ShelfItem[] }`; test helper `mockCommands(handlers: Partial<MockCommandHandlers>)` (installs `mockIPC` with event mocking; a command without a handler rejects). Commands reject with the raw `AppError` object.
- Produces: `App`, `SignInScreen({ expired: boolean })`, `HomeScreen()`, `useAuthStatus(): AuthStatus | null`, `isAppError(value: unknown): value is AppError`, `errorMessage(value: unknown): string`.

- [ ] **Step 1: Write the failing component tests**

`app/src/auth/SignInScreen.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { mockCommands } from "../bindings";
import { SignInScreen } from "./SignInScreen";

test("opens the sign-in window", async () => {
  const signIn = vi.fn(() => null);
  mockCommands({ signIn });
  render(<SignInScreen expired={false} />);

  expect(screen.getByText(/sign in with your google account/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(signIn).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("explains an expired session", () => {
  render(<SignInScreen expired />);
  expect(screen.getByText(/your session expired/i)).toBeInTheDocument();
});

test("shows the error when the window cannot open", async () => {
  mockCommands({
    signIn: () => {
      throw { kind: "internal", message: "could not open the login window" };
    },
  });
  render(<SignInScreen expired={false} />);

  await userEvent.click(screen.getByRole("button", { name: "Sign in with Google" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("could not open the login window");
});
```

Replace `app/src/App.test.tsx`:

```tsx
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { App } from "./App";
import { events, mockCommands, type HomePage } from "./bindings";

const home: HomePage = {
  sections: [
    { title: "Listen again", items: [] },
    {
      title: "Quick picks",
      items: [
        {
          kind: "song",
          video_id: "test-video",
          title: "Test song",
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
  mockCommands({ authStatus: () => "signed_in", getHome: () => home });
  render(<App />);
  expect(await screen.findByText("Listen again (0)")).toBeInTheDocument();
  expect(screen.getByText("Quick picks (1)")).toBeInTheDocument();
});

test("auth://changed switches screens", async () => {
  mockCommands({ authStatus: () => "signed_out", getHome: () => home });
  render(<App />);
  await screen.findByRole("button", { name: "Sign in with Google" });

  await act(() => events.authChanged.emit({ status: "signed_in" }));
  expect(await screen.findByText("Quick picks (1)")).toBeInTheDocument();

  await act(() => events.authChanged.emit({ status: "expired" }));
  expect(await screen.findByText(/your session expired/i)).toBeInTheDocument();
});

test("home errors offer a retry", async () => {
  let calls = 0;
  mockCommands({
    authStatus: () => "signed_in",
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
  mockCommands({ authStatus: () => "signed_in", getHome: () => home, signOut });
  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: "Sign out" }));
  expect(signOut).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd app && pnpm test; cd ..`
Expected: FAIL — `Failed to resolve import "./SignInScreen"` and App tests failing (the placeholder App renders no sign-in button).

- [ ] **Step 3: Implement the modules**

`app/src/errors.ts`:

```ts
import type { AppError } from "./bindings";

/** Commands reject with the Rust `AppError` (`{ kind, message }`). */
export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value &&
    typeof value.message === "string"
  );
}

export function errorMessage(value: unknown): string {
  if (isAppError(value)) return value.message;
  if (value instanceof Error) return value.message;
  return String(value);
}
```

`app/src/auth/useAuthStatus.ts`:

```ts
import { useEffect, useState } from "react";
import { commands, events, type AuthStatus } from "../bindings";

/**
 * The sign-in state: `null` until known, then kept current by `auth://changed`.
 * An event that arrives before the initial `authStatus` reply wins.
 */
export function useAuthStatus(): AuthStatus | null {
  const [status, setStatus] = useState<AuthStatus | null>(null);

  useEffect(() => {
    let active = true;
    const unlisten = events.authChanged.listen((event) => {
      if (active) setStatus(event.payload.status);
    });
    commands.authStatus().then(
      (current) => {
        if (active) setStatus((previous) => previous ?? current);
      },
      () => {
        if (active) setStatus((previous) => previous ?? "signed_out");
      },
    );
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return status;
}
```

`app/src/auth/SignInScreen.tsx`:

```tsx
import { useState } from "react";
import { commands } from "../bindings";
import { errorMessage } from "../errors";

type Props = {
  /** The previous session was rejected by YouTube Music (HTTP 401/403). */
  expired: boolean;
};

export function SignInScreen({ expired }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  async function signIn() {
    setError(null);
    setOpening(true);
    try {
      await commands.signIn();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setOpening(false);
    }
  }

  return (
    <main className="screen">
      <h1>Kaset</h1>
      <p>
        {expired
          ? "Your session expired. Sign in again to continue."
          : "Sign in with your Google account to use YouTube Music."}
      </p>
      <button type="button" onClick={signIn} disabled={opening}>
        Sign in with Google
      </button>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
```

`app/src/home/HomeScreen.tsx`:

```tsx
import { useEffect, useState } from "react";
import { commands, type HomePage } from "../bindings";
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

export function HomeScreen() {
  const [state, setState] = useState<State>({ kind: "loading" });

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

  return (
    <main className="screen">
      <header className="toolbar">
        <h1>Home</h1>
        <button type="button" onClick={signOut}>
          Sign out
        </button>
      </header>
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
          <ul aria-label="Home sections">
            {state.page.sections.map((section, index) => (
              <li key={`${index}-${section.title}`}>
                {section.title} ({section.items.length})
              </li>
            ))}
          </ul>
        ))}
    </main>
  );
}
```

Replace `app/src/App.tsx`:

```tsx
import { SignInScreen } from "./auth/SignInScreen";
import { useAuthStatus } from "./auth/useAuthStatus";
import { HomeScreen } from "./home/HomeScreen";

export function App() {
  const status = useAuthStatus();
  if (status === null) {
    return (
      <main className="screen">
        <p>Loading…</p>
      </main>
    );
  }
  if (status === "signed_in") return <HomeScreen />;
  return <SignInScreen expired={status === "expired"} />;
}
```

Notes for the implementer: `HomeScreen` sets state only in promise callbacks inside the effect (eslint-plugin-react-hooks 7 rejects synchronous `setState` in an effect). `src/test/setup.ts` (Task 3) waits one tick before `clearMocks()`, because the `unlisten` cleanup runs asynchronously after unmount.

- [ ] **Step 4: Run frontend checks**

Run: `cd app && pnpm lint && pnpm typecheck && pnpm test && cd ..`
Expected: lint/typecheck silent; `Test Files 2 passed`, `Tests 8 passed`, and no "Unhandled Rejection" errors.

- [ ] **Step 5: Commit and push**

```bash
git add app/src/errors.ts app/src/auth app/src/home app/src/App.tsx app/src/App.test.tsx
git commit -m "feat(app): add sign-in and home screens"
git push origin main
```

---

### Task 8: CI — Tauri app workflow and bindings drift check

**Files:**
- Modify: `.github/workflows/rust.yml`
- Create: `.github/workflows/app.yml`

**Interfaces:**
- Consumes: Task 3 pnpm scripts (`lint`, `typecheck`, `test`, `tauri`), `app/package.json` `packageManager: pnpm@10.29.1`, `app/pnpm-lock.yaml`, test `bindings_are_current`; Tauri writes bundles to the **workspace** target dir: `target/release/bundle/macos/Kaset.app`, `target/release/bundle/nsis/*-setup.exe`.
- Produces: workflow `App` (job `app`, matrix macOS + Windows) uploading artifact `kaset-windows-nsis`; `Rust` workflow additionally checks `ytm-core` without features and fails if `app/src/bindings.ts` is stale.

- [ ] **Step 1: Update `rust.yml`**

Replace `.github/workflows/rust.yml` with:

```yaml
name: Rust

on:
  push:
    branches: [main, feat/tauri-port]
    paths:
      - "crates/**"
      - "app/src-tauri/**"
      - "app/src/bindings.ts"
      - "Cargo.toml"
      - "Cargo.lock"
      - ".github/workflows/rust.yml"
  pull_request:
    paths:
      - "crates/**"
      - "app/src-tauri/**"
      - "app/src/bindings.ts"
      - "Cargo.toml"
      - "Cargo.lock"
      - ".github/workflows/rust.yml"

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  rust:
    timeout-minutes: 30
    strategy:
      fail-fast: false
      matrix:
        os: [macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      # The workspace build enables ytm-core/specta through the app; check the core alone too.
      - run: cargo clippy -p ytm-core --all-targets -- -D warnings
      - run: cargo test --workspace
      # `cargo test` regenerated app/src/bindings.ts; the committed copy must match.
      - name: Generated bindings are committed
        if: runner.os == 'macOS'
        run: git diff --exit-code -- app/src/bindings.ts
```

- [ ] **Step 2: Create `app.yml`**

`.github/workflows/app.yml`:

```yaml
name: App

on:
  push:
    branches: [main]
    paths:
      - "app/**"
      - "crates/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - ".github/workflows/app.yml"
  pull_request:
    paths:
      - "app/**"
      - "crates/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - ".github/workflows/app.yml"

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  app:
    timeout-minutes: 45
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            bundles: app
          - os: windows-latest
            bundles: nsis
    runs-on: ${{ matrix.os }}
    defaults:
      run:
        working-directory: app
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          package_json_file: app/package.json
      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm
          cache-dependency-path: app/pnpm-lock.yaml
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint
      - run: pnpm typecheck
      - run: pnpm test
      # Unsigned builds. macOS builds only the .app (no DMG); Windows builds the NSIS installer.
      - run: pnpm tauri build --bundles ${{ matrix.bundles }}
      - name: Upload Windows installer
        if: runner.os == 'Windows'
        uses: actions/upload-artifact@v4
        with:
          name: kaset-windows-nsis
          path: target/release/bundle/nsis/*.exe
          if-no-files-found: error
          retention-days: 14
```

(`defaults.run.working-directory` applies to `run` steps only; the upload path is relative to the repository root, where the workspace `target/` lives.)

- [ ] **Step 3: Validate the workflow files locally**

Run:
```bash
python3 -c "import yaml,sys; [yaml.safe_load(open(p)) for p in sys.argv[1:]]; print('ok')" .github/workflows/rust.yml .github/workflows/app.yml
cargo test -p kaset bindings_are_current && git diff --exit-code -- app/src/bindings.ts
```
Expected: `ok`; the bindings diff is empty. (If `python3 -c "import yaml"` fails, run `pip3 install pyyaml` first or skip the YAML parse — GitHub reports syntax errors on push.)

- [ ] **Step 4: Commit, push, and watch CI**

```bash
git add .github/workflows/rust.yml .github/workflows/app.yml
git commit -m "ci: add Tauri app workflow and check generated bindings"
git push origin main
gh run list --limit 4
```

Then wait for the `App` and `Rust` runs on this commit: `gh run watch <run-id> --exit-status` for each. Expected: both succeed on macOS and Windows, and the `App` run on Windows lists the artifact `kaset-windows-nsis` (`gh run view <run-id>`). If a run fails, read `gh run view <run-id> --log-failed`, fix the cause in a follow-up commit with message `ci: fix <what>`, and push again.

---

### Task 9: ADR-0043 and AGENTS.md

**Files:**
- Create: `docs/adr/0043-tauri-app-session-and-ipc.md`
- Modify: `AGENTS.md` (the `## Tauri port (in progress)` section only)

**Interfaces:**
- Consumes: the decisions list at the top of this plan; file and command names from Tasks 3–8.
- Produces: documentation only.

- [ ] **Step 1: Write the ADR**

`docs/adr/0043-tauri-app-session-and-ipc.md`:

```markdown
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
```

- [ ] **Step 2: Update AGENTS.md**

In `AGENTS.md`, replace the whole `## Tauri port (in progress)` section (from that heading down to, not including, the next `## ` heading or end of file) with:

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
- Session storage (ADR-0043): OS secure store via `keyring`. For local debug
  builds, `KASET_SESSION_STORE=file pnpm tauri dev` stores the session in
  `dev-session.txt` in the app data directory instead of the Keychain (debug
  builds only).
- Read WebView cookies with `Webview::cookies()` plus a domain filter, never
  `cookies_for_url`, and only from async code (it deadlocks on Windows otherwise).
- Credentials stay in `ytm_core::secret::Secret`; never log cookie values or
  format `keyring::Error`.
- The Swift-era minimal-dependency rule does not apply to the Rust/Tauri code.
```

- [ ] **Step 3: Check for credential leaks and placeholders**

Run:
```bash
git diff -- AGENTS.md docs/adr/0043-tauri-app-session-and-ipc.md | grep -nE "SAPISID=[A-Za-z0-9_-]{20,}|__Secure-[0-9]PSID=[A-Za-z0-9]" || echo "no credential-like values"
```
Expected: `no credential-like values`.

- [ ] **Step 4: Commit and push**

```bash
git add docs/adr/0043-tauri-app-session-and-ipc.md AGENTS.md
git commit -m "docs: record Tauri app session and IPC decisions (ADR-0043)"
git push origin main
```

---

## Human verification (after Task 9; not a Codex task)

Codex workers cannot sign in to Google. The human runs this on macOS with the YouTube Music Premium account, from `app/`, and records the result in `docs/superpowers/spikes/2026-11-22-tauri-playback.md` under a new "M2 app" heading (names and statuses only, never cookie values):

1. `KASET_SESSION_STORE=file pnpm tauri dev` → the main window shows "Sign in with Google".
2. Click it → the login window opens on accounts.google.com, no "browser may not be secure" page. Complete sign-in → the login window closes and the main window lists home section titles (e.g. "Listen again (n)"). The terminal shows no `blocked login navigation` warning; if it does, note the host (it needs adding to `ALLOWED_ROOTS` or `is_country_google`).
3. Quit and relaunch with the same command → home appears without signing in.
4. Click "Sign out" → sign-in screen. Click "Sign in with Google" → Google asks for the account again (the WebView cookies were deleted).
5. Repeat 1–3 without `KASET_SESSION_STORE` (Keychain path). Allow the Keychain prompt. Confirm `security find-generic-password -s com.sertacozercan.kaset.desktop -a session.count` finds the item (do not add `-w`/`-g`, which print the secret). Sign out and confirm the same command reports that the item could not be found.
6. Windows: install the `kaset-windows-nsis` artifact from the latest `App` run on hardware when available and repeat 2–4 (records spike risks 1 and 4 for WebView2).

## Self-review

- **Spec coverage (milestone 2 + deferred items):** login window to accounts.google.com with Safari UA on macOS, completion on music.youtube.com (Task 6); `Webview::cookies()` + youtube.com suffix filter (Task 5); CookieJar from pairs (Task 1); keyring persistence behind a trait with a debug-only file fallback (Task 4); restore at launch and rotation refresh (Task 6); `auth://changed` (Tasks 3, 6); sign-out clearing store and WebView cookies (Tasks 5, 6); `AuthExpired` → `expired` + UI prompt (Tasks 6, 7); minimal UI listing home section titles through a typed command (Task 7); `app/` scaffold with React/TS/Vite/pnpm/Tauri 2/tauri-specta and workspace membership (Task 3); swappable cookie source (Task 1); specta feature decision (Task 2, ADR in Task 9); `withGlobalTauri: false`, CSP, login navigation allowlist, typed IPC payloads, capability only for `main` (Tasks 3, 6); CI Tauri build matrix with Windows installer artifact and `permissions: contents: read` (Task 8). Player-window hardening (typed bounded `playback_event`, `videoId` validation, player navigation allowlist) stays in M3, reusing `is_allowed_login_navigation`'s pattern and the capability layout from Task 3.
- **Placeholder scan:** every code step has complete code; no TBD/TODO.
- **Type consistency:** `SharedCookies::{new, get, set}` (Task 1) used by `Session` (Task 5) and `AppState::client` (Task 6); `Session::sign_in/refresh/mark_expired/sign_out` return types match their call sites in `login.rs`, `commands.rs`, `lib.rs`; `store_kind(Option<&str>, bool)` / `store_for(StoreKind, &Path)` match `lib.rs`; TS command names `authStatus`, `signIn`, `signOut`, `getHome` and the event `authChanged` match the generated bindings verified with tauri-specta rc.29.
- **Verified before writing:** the code of Tasks 1–7 was compiled and tested in a scratch copy of this workspace (ytm-core tests, 33 `kaset` tests, clippy `-D warnings`, 8 Vitest tests, ESLint, `tsc -b`) and `pnpm tauri build --bundles app` produced `Kaset.app` on macOS. Not verified: runtime behaviour of the login window, Keychain access, and Windows (left to the human checklist and CI).
