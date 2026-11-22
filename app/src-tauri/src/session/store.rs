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
            [
                "session.a.0",
                "session.a.1",
                "session.a.2",
                "session.current"
            ]
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
            [
                "session.b.0",
                "session.b.1",
                "session.b.2",
                "session.current"
            ]
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
                        backend
                            .set(&Slot::Legacy.chunk(index), b"test-cookie")
                            .unwrap();
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
