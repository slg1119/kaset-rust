//! Sign-in state and cookie ownership. No Tauri types: the window and event
//! glue lives in `login.rs` and `commands.rs`, so this is testable alone.

pub mod cookies;
mod status;
pub mod store;

pub use status::AuthStatus;

use std::sync::{Mutex, MutexGuard, PoisonError};

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
/// A return value of `Some(new_status)` signals that the caller should emit
/// `auth://changed`.
pub struct Session {
    cookies: SharedCookies,
    status: Mutex<AuthStatus>,
    store: Box<dyn SessionStore>,
    // Serializes status decisions, store I/O, and shared cookie updates.
    transition: Mutex<()>,
}

impl Session {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            cookies: SharedCookies::default(),
            status: Mutex::new(AuthStatus::SignedOut),
            store,
            transition: Mutex::new(()),
        }
    }

    /// The handle to give `InnerTubeClient::with_shared_cookies`.
    pub fn cookies(&self) -> &SharedCookies {
        &self.cookies
    }

    pub fn status(&self) -> AuthStatus {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_transition(&self) -> MutexGuard<'_, ()> {
        self.transition
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
        let _transition = self.lock_transition();
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
        let _transition = self.lock_transition();
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
        let _transition = self.lock_transition();
        if self.status() != AuthStatus::SignedIn {
            return None;
        }
        self.set_status(AuthStatus::Expired)
    }

    /// Forgets the session in memory and in the store. The in-memory state is
    /// cleared even when the store fails, and the error is returned.
    pub fn sign_out(&self) -> Result<Option<AuthStatus>, SessionError> {
        let _transition = self.lock_transition();
        self.cookies.set(None);
        let changed = self.set_status(AuthStatus::SignedOut);
        self.store.clear()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver, Sender};
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
            Some(Secret::new(
                "SAPISID=test-sapisid-rotated; HSID=test-cookie"
            ))
        );
        assert!(!session.refresh(CookieJar::from_pairs([("HSID", "test-cookie")])));
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
}
