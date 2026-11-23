use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, State};

use crate::error::AppError;
use crate::webview_session::blocking;

pub(crate) struct TrackNotifications {
    data_dir: PathBuf,
    enabled: AtomicBool,
    write: Mutex<()>,
}

const FILE_NAME: &str = "track-notifications.json";

impl TrackNotifications {
    pub(crate) fn load(data_dir: &Path) -> io::Result<Self> {
        let enabled = match fs::read(data_dir.join(FILE_NAME)) {
            Ok(contents) => serde_json::from_slice(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => return Err(error),
        };
        Ok(Self {
            data_dir: data_dir.to_owned(),
            enabled: AtomicBool::new(enabled),
            write: Mutex::new(()),
        })
    }

    pub(crate) fn restore(data_dir: &Path) -> Self {
        Self::load(data_dir).unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load track notification setting");
            Self {
                data_dir: data_dir.to_owned(),
                enabled: AtomicBool::new(true),
                write: Mutex::new(()),
            }
        })
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set(&self, enabled: bool) -> io::Result<()> {
        let _write = self.write.lock().expect("track notification write mutex");
        fs::create_dir_all(&self.data_dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&self.data_dir)?;
        serde_json::to_writer(file.as_file_mut(), &enabled)?;
        file.as_file().sync_all()?;
        file.persist(self.data_dir.join(FILE_NAME))
            .map_err(|error| error.error)?;
        self.enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }
}

/// Whether the app sends notifications when playback changes tracks.
#[tauri::command]
#[specta::specta]
pub async fn track_notifications(
    settings: State<'_, TrackNotifications>,
) -> Result<bool, AppError> {
    Ok(settings.enabled())
}

/// Saves the track notification preference for future launches.
#[tauri::command]
#[specta::specta]
pub async fn set_track_notifications(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    blocking(move || app.state::<TrackNotifications>().set(enabled))
        .await?
        .map_err(|error| {
            tracing::warn!(%error, "could not save track notification setting");
            AppError::internal("Could not save the track notification setting.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task13_track_notifications_default_to_on_and_survive_reload() {
        let dir = tempfile::tempdir().unwrap();
        let settings = TrackNotifications::load(dir.path()).unwrap();
        assert!(settings.enabled());
        settings.set(false).unwrap();
        assert!(!settings.enabled());
        let settings = TrackNotifications::load(dir.path()).unwrap();
        assert!(!settings.enabled());
        settings.set(true).unwrap();
        assert!(TrackNotifications::load(dir.path()).unwrap().enabled());
    }

    #[test]
    fn task13_track_notifications_keep_the_previous_value_after_a_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let settings = TrackNotifications::load(&data_dir).unwrap();
        std::fs::write(&data_dir, b"test").unwrap();
        assert!(settings.set(false).is_err());
        assert!(settings.enabled());
    }
}
