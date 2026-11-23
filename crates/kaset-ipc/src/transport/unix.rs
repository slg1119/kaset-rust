use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use tokio::net::{UnixListener, UnixStream};

use super::{ClientError, Listener};

pub(super) fn endpoint() -> io::Result<PathBuf> {
    Ok(std::env::temp_dir()
        .join(format!("kaset-{}", uid()))
        .join("kaset.sock"))
}

fn uid() -> u32 {
    // SAFETY: geteuid takes no arguments and has no preconditions.
    unsafe { libc::geteuid() }
}

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "unsafe Kaset socket path or peer",
    )
}

fn checked_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid() || metadata.mode() & 0o777 != 0o700 {
        return Err(denied());
    }
    Ok(())
}

fn checked_socket(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket() || metadata.uid() != uid() {
        return Err(denied());
    }
    Ok(())
}

fn parent(path: &Path) -> &Path {
    path.parent().expect("Kaset socket has a parent directory")
}

pub(super) struct Server {
    listener: UnixListener,
    path: PathBuf,
    // Hold the lock through socket cleanup. The lock file itself stays in place.
    _lock: File,
}

impl Server {
    pub(super) fn bind(path: &Path) -> io::Result<Self> {
        let directory = parent(path);
        match DirBuilder::new().mode(0o700).create(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        checked_directory(directory)?;

        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(directory.join("listener.lock"))?;
        let metadata = lock.metadata()?;
        if !metadata.is_file() || metadata.uid() != uid() || metadata.mode() & 0o077 != 0 {
            return Err(denied());
        }
        // SAFETY: lock owns the valid descriptor for the lifetime of the listener.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }

        match checked_socket(path) {
            Ok(()) => fs::remove_file(path)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let server = Self {
            listener: UnixListener::bind(path)?,
            path: path.to_owned(),
            _lock: lock,
        };
        fs::set_permissions(path, Permissions::from_mode(0o600))?;
        Ok(server)
    }
}

impl Listener for Server {
    type Stream = UnixStream;

    async fn accept(&mut self) -> io::Result<Self::Stream> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            if stream.peer_cred().is_ok_and(|peer| peer.uid() == uid()) {
                return Ok(stream);
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) async fn connect(path: &Path) -> Result<UnixStream, ClientError> {
    let stream = async {
        checked_directory(parent(path))?;
        checked_socket(path)?;
        UnixStream::connect(path).await
    }
    .await
    .map_err(|error| match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => ClientError::NotRunning,
        _ => error.into(),
    })?;
    if stream.peer_cred()?.uid() != uid() {
        return Err(denied().into());
    }
    Ok(stream)
}
