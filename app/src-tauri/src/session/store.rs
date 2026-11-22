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
const MAX_SWEEP_CHUNKS: usize = 64;

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

    fn sweep_end(&self, start: usize) -> Result<usize, StoreError> {
        let end = start.saturating_add(MAX_SWEEP_CHUNKS);
        for index in start..end {
            if self.backend.get(&chunk_entry(index))?.is_none() {
                return Ok(index);
            }
        }
        Ok(end)
    }

    fn delete_chunks(&self, chunks: std::ops::Range<usize>) -> Result<(), StoreError> {
        let mut first_error = None;
        for index in chunks {
            if let Err(error) = self.backend.delete(&chunk_entry(index)) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
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
        let old_count = self.count();
        let chunks = header.expose().as_bytes().chunks(CHUNK_SIZE);
        let new_count = chunks.len();
        for (index, chunk) in chunks.enumerate() {
            self.backend.set(&chunk_entry(index), chunk)?;
        }
        // A mid-write failure can leave a mix of old and new chunks. This can
        // surface later as an authentication failure and require signing in again.
        self.backend
            .set(COUNT_ENTRY, new_count.to_string().as_bytes())?;
        let trim_end = match old_count {
            Ok(Some(count)) => Ok(count),
            Ok(None) | Err(_) => self.sweep_end(new_count),
        };
        if trim_end
            .and_then(|end| self.delete_chunks(new_count..end))
            .is_err()
        {
            tracing::warn!("Failed to trim stale session chunks");
        }
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        let count = self.count().ok().flatten().unwrap_or(0);
        let end = self.sweep_end(count)?;
        if end > count {
            // Keep the full range available for a retry if deletion leaves gaps.
            self.backend.set(COUNT_ENTRY, end.to_string().as_bytes())?;
        }
        self.delete_chunks(0..end)?;
        self.backend.delete(COUNT_ENTRY)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct MapBackend {
        entries: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
        fail_delete_once: Arc<Mutex<Option<&'static str>>>,
    }

    impl MapBackend {
        fn names(&self) -> Vec<String> {
            self.entries.lock().unwrap().keys().cloned().collect()
        }

        fn fail_next_delete_of(&self, name: &'static str) {
            *self.fail_delete_once.lock().unwrap() = Some(name);
        }
    }

    impl EntryBackend for MapBackend {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
            Ok(self.entries.lock().unwrap().get(name).cloned())
        }
        fn set(&self, name: &str, value: &[u8]) -> Result<(), StoreError> {
            self.entries
                .lock()
                .unwrap()
                .insert(name.into(), value.to_vec());
            Ok(())
        }
        fn delete(&self, name: &str) -> Result<(), StoreError> {
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
    fn chunked_store_sweeps_stale_chunks_when_old_count_is_corrupt() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        backend.set(COUNT_ENTRY, b"invalid").unwrap();
        let header = Secret::new("SAPISID=test-sapisid");
        store.save(&header).unwrap();
        assert_eq!(store.load().unwrap(), Some(header));
        assert_eq!(backend.names(), ["session.0", "session.count"]);
    }

    #[test]
    fn chunked_store_save_succeeds_when_stale_chunk_deletion_fails() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        backend.fail_next_delete_of("session.1");
        let header = Secret::new("SAPISID=test-sapisid");
        store.save(&header).unwrap();
        assert_eq!(store.load().unwrap(), Some(header));
        assert_eq!(backend.names(), ["session.0", "session.1", "session.count"]);
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
    fn chunked_store_clear_sweeps_chunks_beyond_a_valid_count() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        backend.set(COUNT_ENTRY, b"2").unwrap();
        for index in 0..4 {
            backend.set(&chunk_entry(index), b"test-cookie").unwrap();
        }
        store.clear().unwrap();
        assert!(backend.names().is_empty());
    }

    #[test]
    fn chunked_store_clear_can_retry_failed_stale_chunk_deletions() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        backend.set(COUNT_ENTRY, b"2").unwrap();
        for index in 0..4 {
            backend.set(&chunk_entry(index), b"test-cookie").unwrap();
        }
        backend.fail_next_delete_of("session.3");

        assert!(matches!(
            store.clear(),
            Err(StoreError::Keyring("platform failure"))
        ));
        assert_eq!(backend.names(), ["session.3", "session.count"]);
        store.clear().unwrap();
        assert!(backend.names().is_empty());
    }

    #[test]
    fn chunked_store_clear_sweeps_chunks_when_count_is_corrupt() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
        backend.set(COUNT_ENTRY, b"invalid").unwrap();
        store.clear().unwrap();
        assert!(backend.names().is_empty());
    }

    #[test]
    fn chunked_store_clear_sweeps_chunks_when_count_is_missing() {
        let backend = MapBackend::default();
        let store = ChunkedStore::new(backend.clone());
        store.save(&long_header(CHUNK_SIZE * 2)).unwrap();
        backend.delete(COUNT_ENTRY).unwrap();
        store.clear().unwrap();
        assert!(backend.names().is_empty());
    }

    #[test]
    fn chunked_store_clear_can_retry_failed_chunk_deletions() {
        for count in [Some(b"3".as_slice()), Some(b"invalid".as_slice()), None] {
            let backend = MapBackend::default();
            let store = ChunkedStore::new(backend.clone());
            store.save(&long_header(CHUNK_SIZE * 3)).unwrap();
            match count {
                Some(raw) => backend.set(COUNT_ENTRY, raw).unwrap(),
                None => backend.delete(COUNT_ENTRY).unwrap(),
            }
            backend.fail_next_delete_of("session.1");

            assert!(matches!(
                store.clear(),
                Err(StoreError::Keyring("platform failure"))
            ));
            assert_eq!(backend.names(), ["session.1", "session.count"]);
            store.clear().unwrap();
            assert!(backend.names().is_empty());
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
