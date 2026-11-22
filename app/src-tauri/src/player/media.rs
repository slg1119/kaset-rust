//! OS media controls through souvlaki: macOS Now Playing and media keys,
//! Windows System Media Transport Controls (SMTC) and media keys.
//!
//! souvlaki's handle is created, updated, and dropped on the main thread only:
//! on macOS it registers `MPRemoteCommandCenter` handlers, and on Windows it
//! binds SMTC to the main window's HWND. The handle lives in a main-thread
//! thread-local and every call goes through `AppHandle::run_on_main_thread`.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Manager, Url};

use super::engine::{Outcome, PlaybackStatus, Player, PlayerState};
use crate::state::AppState;

/// Seek step for media keys that do not say how far (Windows fast-forward/rewind).
pub const SEEK_STEP_SECONDS: f64 = 10.0;

thread_local! {
    static CONTROLS: RefCell<Option<MediaControls>> = const { RefCell::new(None) };
}

/// Track metadata the OS shows.
#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
}

/// Playback state the OS shows, with a validated position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Playback {
    Stopped,
    Paused(Duration),
    Playing(Duration),
}

/// A media key or OS control, mapped to a player change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaAction {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    SeekTo(f64),
    SeekBy(f64),
}

impl MediaAction {
    pub fn from_event(event: &MediaControlEvent) -> Option<Self> {
        let step = |direction: &SeekDirection, seconds: f64| match direction {
            SeekDirection::Forward => seconds,
            SeekDirection::Backward => -seconds,
        };
        Some(match event {
            MediaControlEvent::Play => Self::Play,
            MediaControlEvent::Pause | MediaControlEvent::Stop => Self::Pause,
            MediaControlEvent::Toggle => Self::Toggle,
            MediaControlEvent::Next => Self::Next,
            MediaControlEvent::Previous => Self::Previous,
            MediaControlEvent::SetPosition(MediaPosition(position)) => {
                Self::SeekTo(position.as_secs_f64())
            }
            MediaControlEvent::SeekBy(direction, amount) => {
                Self::SeekBy(step(direction, amount.as_secs_f64()))
            }
            MediaControlEvent::Seek(direction) => Self::SeekBy(step(direction, SEEK_STEP_SECONDS)),
            _ => return None,
        })
    }

    pub fn apply(self, player: &mut Player, now: Instant) -> Outcome {
        match self {
            Self::Play => player.play(now),
            Self::Pause => player.pause(now),
            Self::Toggle => player.toggle(now),
            Self::Next => player.next(now),
            Self::Previous => player.previous(now),
            Self::SeekTo(position) => player.seek(position, now),
            Self::SeekBy(delta) => {
                let position = player.state().position;
                player.seek(position + delta, now)
            }
        }
    }
}

pub fn now_playing(state: &PlayerState) -> Option<NowPlaying> {
    let track = state.track.as_ref()?;
    let artist = track
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(NowPlaying {
        title: track.title.clone(),
        artist,
        cover_url: track
            .thumbnail_url
            .as_deref()
            .and_then(|url| Url::parse(url).ok())
            .filter(|url| url.scheme() == "https")
            .map(Into::into),
        duration: if state.duration > 0.0 && !state.is_ad {
            Duration::try_from_secs_f64(state.duration).ok()
        } else {
            None
        },
    })
}

pub fn playback(state: &PlayerState) -> Playback {
    let position = if state.is_ad {
        Duration::ZERO
    } else {
        Duration::try_from_secs_f64(state.position).unwrap_or_default()
    };
    match state.status {
        PlaybackStatus::Idle => Playback::Stopped,
        PlaybackStatus::Playing => Playback::Playing(position),
        PlaybackStatus::Loading | PlaybackStatus::Paused | PlaybackStatus::Ended => {
            Playback::Paused(position)
        }
    }
}

/// What to send to the OS for one player state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaUpdate {
    /// `None` leaves metadata unchanged, and `Some(None)` clears it.
    pub metadata: Option<Option<NowPlaying>>,
    pub playback: Option<Playback>,
}

/// Remembers what the OS was last told, so steady progress does not resend
/// anything: the OS advances a playing position on its own.
#[derive(Debug, Default)]
pub struct MediaPublisher {
    metadata: Option<NowPlaying>,
    playback: Option<(Playback, Instant)>,
}

/// A playing position further than this from the extrapolated one is a seek.
const DRIFT_SECONDS: f64 = 1.5;

impl MediaPublisher {
    pub fn update(&mut self, state: &PlayerState, now: Instant) -> MediaUpdate {
        let mut update = MediaUpdate::default();
        let metadata = now_playing(state);
        if metadata != self.metadata {
            update.metadata = Some(metadata.clone());
            self.metadata = metadata;
            // New metadata resets the position the OS shows.
            self.playback = None;
        }
        let next = playback(state);
        let changed = match (self.playback, next) {
            (None, _) => true,
            (Some((Playback::Playing(was), at)), Playback::Playing(is)) => {
                let expected = was.as_secs_f64() + now.saturating_duration_since(at).as_secs_f64();
                (is.as_secs_f64() - expected).abs() > DRIFT_SECONDS
            }
            (Some((Playback::Paused(was), _)), Playback::Paused(is)) => {
                was.abs_diff(is) > Duration::from_millis(500)
            }
            (Some((previous, _)), next) => {
                std::mem::discriminant(&previous) != std::mem::discriminant(&next)
            }
        };
        if changed {
            update.playback = Some(next);
            self.playback = Some((next, now));
        }
        update
    }
}

/// Creates the OS media controls on the main thread and routes their events
/// to the player. Failure is logged; the app works without media keys.
pub fn attach(app: &AppHandle) {
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        #[cfg(windows)]
        let hwnd = match handle
            .get_webview_window("main")
            .map(|window| window.hwnd())
        {
            Some(Ok(hwnd)) => Some(hwnd.0),
            _ => {
                tracing::warn!("no main window handle; media controls disabled");
                return;
            }
        };
        #[cfg(not(windows))]
        let hwnd = None;
        let config = PlatformConfig {
            display_name: "Kaset",
            dbus_name: "kaset",
            hwnd,
        };
        let mut controls = match MediaControls::new(config) {
            Ok(controls) => controls,
            Err(error) => {
                tracing::warn!(?error, "could not create media controls");
                return;
            }
        };
        let events_app = handle.clone();
        let attached = controls.attach(move |event| {
            let Some(action) = MediaAction::from_event(&event) else {
                return;
            };
            let app = events_app.clone();
            // Handlers run on the main thread on macOS; never lock the player there.
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                if let Err(error) = state
                    .player
                    .update(&app, |player, now| Ok(action.apply(player, now)))
                    .await
                {
                    tracing::warn!(%error, "media control action failed");
                }
            });
        });
        if let Err(error) = attached {
            tracing::warn!(?error, "could not attach media controls");
            return;
        }
        CONTROLS.with(|slot| *slot.borrow_mut() = Some(controls));
    });
    if let Err(error) = result {
        tracing::warn!(%error, "could not schedule media controls setup");
    }
}

#[cfg(windows)]
fn clear_windows_metadata(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use windows::Media::{MediaPlaybackType, SystemMediaTransportControls};
    use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

    let window = app.get_webview_window("main").ok_or("no main window")?;
    let hwnd = window.hwnd()?;
    let interop = windows::core::factory::<
        SystemMediaTransportControls,
        ISystemMediaTransportControlsInterop,
    >()?;
    // SAFETY: This is the live HWND used by souvlaki, accessed on its owning thread.
    let controls: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd) }?;
    let updater = controls.DisplayUpdater()?;
    updater.ClearAll()?;
    updater.SetType(MediaPlaybackType::Music)?;
    Ok(())
}

/// Sends `update` to the OS on the main thread.
pub fn publish(app: &AppHandle, update: MediaUpdate) {
    if update == MediaUpdate::default() {
        return;
    }
    #[cfg(windows)]
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        CONTROLS.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(controls) = slot.as_mut() else {
                return;
            };
            if let Some(metadata) = &update.metadata {
                // On Windows, souvlaki leaves fields unchanged when they are omitted.
                #[cfg(windows)]
                if metadata.is_none()
                    && let Err(error) = clear_windows_metadata(&handle)
                {
                    tracing::warn!(%error, "could not clear Windows media metadata");
                }
                let metadata = metadata
                    .as_ref()
                    .map_or_else(MediaMetadata::default, |metadata| MediaMetadata {
                        title: Some(&metadata.title),
                        artist: Some(&metadata.artist),
                        album: None,
                        cover_url: metadata.cover_url.as_deref(),
                        duration: metadata.duration,
                    });
                if let Err(error) = controls.set_metadata(metadata) {
                    tracing::warn!(?error, "could not update media metadata");
                }
            }
            if let Some(playback) = update.playback {
                let playback = match playback {
                    Playback::Stopped => MediaPlayback::Stopped,
                    Playback::Paused(position) => MediaPlayback::Paused {
                        progress: Some(MediaPosition(position)),
                    },
                    Playback::Playing(position) => MediaPlayback::Playing {
                        progress: Some(MediaPosition(position)),
                    },
                };
                if let Err(error) = controls.set_playback(playback) {
                    tracing::warn!(?error, "could not update media playback state");
                }
            }
        });
    });
    if let Err(error) = result {
        tracing::warn!(%error, "could not schedule a media controls update");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::queue::RepeatMode;
    use ytm_core::models::{ArtistRef, Song};

    fn state(status: PlaybackStatus, position: f64) -> PlayerState {
        PlayerState {
            status,
            track: Some(Song {
                video_id: "test-vid-0A".into(),
                title: "Test song".into(),
                artists: vec![
                    ArtistRef {
                        name: "Artist one".into(),
                        browse_id: None,
                    },
                    ArtistRef {
                        name: "Artist two".into(),
                        browse_id: None,
                    },
                ],
                thumbnail_url: Some("https://lh3.googleusercontent.com/test".into()),
            }),
            position,
            duration: 200.0,
            is_ad: false,
            repeat: RepeatMode::Off,
            shuffle: false,
            has_previous: false,
            has_next: false,
        }
    }

    #[test]
    fn maps_media_events() {
        use MediaControlEvent as E;
        assert_eq!(
            MediaAction::from_event(&E::Toggle),
            Some(MediaAction::Toggle)
        );
        assert_eq!(MediaAction::from_event(&E::Stop), Some(MediaAction::Pause));
        assert_eq!(
            MediaAction::from_event(&E::SetPosition(MediaPosition(Duration::from_secs(42)))),
            Some(MediaAction::SeekTo(42.0))
        );
        assert_eq!(
            MediaAction::from_event(&E::SeekBy(SeekDirection::Backward, Duration::from_secs(5))),
            Some(MediaAction::SeekBy(-5.0))
        );
        assert_eq!(
            MediaAction::from_event(&E::Seek(SeekDirection::Forward)),
            Some(MediaAction::SeekBy(SEEK_STEP_SECONDS))
        );
        assert_eq!(MediaAction::from_event(&E::Raise), None);
        assert_eq!(
            MediaAction::from_event(&E::OpenUri("https://example.com".into())),
            None
        );
    }

    #[test]
    fn now_playing_joins_artists_and_hides_ad_duration() {
        let info = now_playing(&state(PlaybackStatus::Playing, 1.0)).unwrap();
        assert_eq!(info.title, "Test song");
        assert_eq!(info.artist, "Artist one, Artist two");
        assert_eq!(info.duration, Some(Duration::from_secs(200)));
        let mut ad = state(PlaybackStatus::Playing, 1.0);
        ad.is_ad = true;
        assert_eq!(now_playing(&ad).unwrap().duration, None);
        assert_eq!(playback(&ad), Playback::Playing(Duration::ZERO));
    }

    #[test]
    fn unrepresentable_duration_is_omitted_and_position_is_zero() {
        let mut state = state(PlaybackStatus::Playing, 1e30);
        state.duration = 1e30;
        assert_eq!(now_playing(&state).unwrap().duration, None);
        assert_eq!(playback(&state), Playback::Playing(Duration::ZERO));
        state.status = PlaybackStatus::Paused;
        assert_eq!(playback(&state), Playback::Paused(Duration::ZERO));
    }

    #[test]
    fn now_playing_only_includes_valid_https_artwork() {
        let mut state = state(PlaybackStatus::Playing, 1.0);
        assert_eq!(
            now_playing(&state).unwrap().cover_url.as_deref(),
            Some("https://lh3.googleusercontent.com/test")
        );
        for url in ["http://example.com/cover", "file:///cover", "https://["] {
            state.track.as_mut().unwrap().thumbnail_url = Some(url.into());
            assert_eq!(now_playing(&state).unwrap().cover_url, None);
        }
    }

    #[test]
    fn publisher_sends_metadata_once_and_skips_steady_progress() {
        let mut publisher = MediaPublisher::default();
        let start = Instant::now();
        let first = publisher.update(&state(PlaybackStatus::Playing, 10.0), start);
        assert!(matches!(first.metadata, Some(Some(_))));
        assert_eq!(
            first.playback,
            Some(Playback::Playing(Duration::from_secs(10)))
        );

        let later = start + Duration::from_secs(2);
        assert_eq!(
            publisher.update(&state(PlaybackStatus::Playing, 12.0), later),
            MediaUpdate::default()
        );
        // A seek moves the position away from the extrapolated one.
        let seeked = publisher.update(&state(PlaybackStatus::Playing, 90.0), later);
        assert_eq!(
            seeked.playback,
            Some(Playback::Playing(Duration::from_secs(90)))
        );
        assert!(seeked.metadata.is_none());
        // Pausing is a change.
        let paused = publisher.update(&state(PlaybackStatus::Paused, 90.0), later);
        assert_eq!(
            paused.playback,
            Some(Playback::Paused(Duration::from_secs(90)))
        );
    }

    #[test]
    fn publisher_reports_stop_when_the_queue_empties() {
        let mut publisher = MediaPublisher::default();
        let now = Instant::now();
        publisher.update(&state(PlaybackStatus::Playing, 1.0), now);
        let mut idle = state(PlaybackStatus::Idle, 0.0);
        idle.track = None;
        let update = publisher.update(&idle, now);
        assert_eq!(update.metadata, Some(None));
        assert_eq!(update.playback, Some(Playback::Stopped));
        assert_eq!(publisher.update(&idle, now), MediaUpdate::default());
    }

    #[test]
    fn seek_by_is_relative_to_the_current_position() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player
            .play_songs(
                vec![Song {
                    video_id: "test-vid-0A".into(),
                    title: "Test song".into(),
                    artists: Vec::new(),
                    thumbnail_url: None,
                }],
                0,
                now,
            )
            .unwrap();
        player.handle_event(
            crate::player::engine::PlaybackEvent::Progress {
                video_id: crate::player::video_id::VideoId::parse("test-vid-0A").unwrap(),
                playing: true,
                position: 30.0,
                duration: 200.0,
                ad: false,
            },
            now,
        );
        let outcome = MediaAction::SeekBy(-10.0).apply(&mut player, now);
        assert_eq!(
            outcome.command,
            Some(crate::player::engine::WebCommand::Seek(20.0))
        );
    }
}
