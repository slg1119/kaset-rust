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
    /// WebView cookies, an API request) and pass it to the corresponding
    /// epoch-guarded operation. Never blocks on store I/O.
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
            if state.status == AuthStatus::SignedIn && self.cookies.get().as_ref() == Some(&jar) {
                return Ok(None);
            }
        }
        if let Err(error) = self.store.save(jar.header()) {
            tracing::warn!(%error, "signed in, but the session could not be saved");
        }
        self.cookies.set(Some(jar));
        Ok(self.set_status(AuthStatus::SignedIn, true))
    }

    /// `sign_in_if` with the current epoch.
    #[cfg(test)]
    pub fn sign_in(&self, jar: CookieJar) -> Result<Option<AuthStatus>, SessionError> {
        self.sign_in_if(self.epoch(), jar)
    }

    /// `refresh_if` with the current epoch.
    #[cfg(test)]
    pub fn refresh(&self, jar: CookieJar) -> bool {
        self.refresh_if(self.epoch(), jar)
    }

    /// Picks up rotated WebView cookies for the current signed-in session.
    /// Only changes to the auth cookie set trigger a save. Rotation keeps the
    /// same epoch.
    pub fn refresh_if(&self, epoch: u64, jar: CookieJar) -> bool {
        let _transition = self.lock_transition();
        {
            let state = self.lock_state();
            if state.epoch != epoch || state.status != AuthStatus::SignedIn {
                return false;
            }
        }
        if jar.sapisid().is_none() {
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
        let store = CountingStore::default();
        let saves = Arc::clone(&store.saves);
        let session = Session::new(Box::new(store));
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
        let epoch = session.epoch();
        assert_eq!(session.sign_in(jar("test-sapisid")).unwrap(), None);
        assert_eq!(session.epoch(), epoch);
        assert_eq!(saves.load(Ordering::Relaxed), 1);
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
    fn stale_refresh_cannot_overwrite_a_newer_session() {
        let (session, saves) = restored_counting_session(&jar("test-old"));
        let epoch = session.epoch();
        session.sign_out().unwrap();
        let new = jar("test-new");
        session.sign_in(new.clone()).unwrap();

        assert!(!session.refresh_if(epoch, jar("test-old-rotated")));
        assert_eq!(session.cookies().get().as_ref(), Some(&new));
        assert_eq!(session.store.load().unwrap().as_ref(), Some(new.header()));
        assert_eq!(saves.load(Ordering::Relaxed), 1);
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
