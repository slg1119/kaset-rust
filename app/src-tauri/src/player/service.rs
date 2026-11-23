//! Applies engine outcomes: drives the player window, emits `player://state`
//! and `player://queue` to the main window, and updates the OS media controls.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::Event;
use tokio::sync::{Mutex, MutexGuard};
use ytm_core::models::RadioQueue;
use ytm_core::parsers::radio::{parse_radio, parse_radio_continuation};

use super::autoplay::{Autoplay, Ticket};
use super::engine::{Outcome, PlaybackStatus, Player, PlayerState, QueueState, Seed, WebCommand};
use super::media::{self, MediaPublisher};
use super::video_id::VideoId;
use super::window;
use crate::commands::on_error;
use crate::error::AppError;
use crate::events::{PlayerRedirected, PlayerStateChanged, QueueChanged};
use crate::session::AuthStatus;
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::track_notifications::TrackNotifications;

/// Player events go to the main window. The player window's capability has
/// no event permissions, so its pages cannot listen to them either.
const UI_WINDOW: &str = "main";

struct Inner {
    player: Player,
    window: Option<window::PlayerWindow>,
    media: MediaPublisher,
    last_notified_id: Option<String>,
    autoplay: Autoplay,
}

impl Inner {
    async fn apply(
        &mut self,
        app: &AppHandle,
        mut outcome: Outcome,
    ) -> (Result<(), AppError>, Option<TrackNotification>) {
        let result = match outcome.command.take() {
            Some(WebCommand::Load(id)) => {
                let generation = self.player.load_generation();
                let result = self.load(app, &id, generation).await;
                if result.is_err() {
                    outcome.state = self.player.fail_load(generation, Instant::now()).state;
                }
                result
            }
            Some(WebCommand::Play) => window::run_script(app, window::play_script()),
            Some(WebCommand::Pause) => window::run_script(app, window::pause_script()),
            Some(WebCommand::Seek(position)) => {
                window::run_script(app, &window::seek_script(position))
            }
            Some(WebCommand::Stop) => {
                self.close_window(app);
                Ok(())
            }
            None => Ok(()),
        };
        (result, self.publish(app, outcome, Instant::now()))
    }

    async fn load(
        &mut self,
        app: &AppHandle,
        id: &VideoId,
        generation: u64,
    ) -> Result<(), AppError> {
        let window = match &mut self.window {
            Some(window) => window,
            slot @ None => slot.insert(window::PlayerWindow::new(app)?),
        };
        window.load(id, generation).await
    }

    fn close_window(&mut self, app: &AppHandle) {
        self.window = None;
        window::close(app);
    }

    fn publish(
        &mut self,
        app: &AppHandle,
        outcome: Outcome,
        now: Instant,
    ) -> Option<TrackNotification> {
        let notification = outcome.state.as_ref().and_then(|state| {
            let update = self.media.update(state, now);
            media::publish(app, update);
            track_notification(&mut self.last_notified_id, state)
        });
        if let Some(queue) = outcome.queue
            && let Err(error) = (QueueChanged { queue }).emit_to(app, UI_WINDOW)
        {
            tracing::warn!(%error, "could not emit player://queue");
        }
        if let Some(state) = outcome.state
            && let Err(error) = (PlayerStateChanged { state }).emit_to(app, UI_WINDOW)
        {
            tracing::warn!(%error, "could not emit player://state");
        }
        notification
    }
}

pub struct PlayerService {
    // A tokio mutex, held while an outcome is applied, so window commands run
    // in the order the engine produced them. Never locked on the main thread.
    inner: Mutex<Inner>,
}

pub(crate) struct SessionPlayerGuard<'a> {
    inner: MutexGuard<'a, Inner>,
}

impl SessionPlayerGuard<'_> {
    pub(crate) fn stop(&mut self, app: &AppHandle) {
        let now = Instant::now();
        let outcome = self.inner.player.stop(now);
        self.inner.autoplay.cancel_pending();
        self.inner.close_window(app);
        self.inner.publish(app, outcome, now);
    }
}

impl PlayerService {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1);
        Self {
            inner: Mutex::new(Inner {
                player: Player::new(seed),
                window: None,
                media: MediaPublisher::default(),
                last_notified_id: None,
                autoplay: Autoplay::default(),
            }),
        }
    }

    /// Runs `change` on the engine and applies its outcome.
    pub async fn update<F>(&self, app: &AppHandle, change: F) -> Result<(), AppError>
    where
        F: FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>,
    {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let previous = inner.player.autoplay_trigger();
        let outcome = change(&mut inner.player, now)?;
        let (result, notification) = inner.apply(app, outcome).await;
        if previous != inner.player.autoplay_trigger() {
            inner.autoplay.request();
            start_autoplay(app, &mut inner);
        }
        drop(inner);
        send_track_notification(app, notification);
        result
    }

    pub(super) async fn redirected(&self, app: &AppHandle, generation: u64) {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let outcome = inner.player.fail_load(generation, now);
        if outcome.state.is_none() {
            return;
        }
        inner.publish(app, outcome, now);
        if let Err(error) = (PlayerRedirected {}).emit_to(app, UI_WINDOW) {
            tracing::warn!(%error, "could not emit player://redirected");
        }
    }

    pub(crate) async fn queue_generation(&self) -> u64 {
        self.inner.lock().await.player.queue_position().0
    }

    pub async fn snapshot(&self) -> (PlayerState, QueueState) {
        let inner = self.inner.lock().await;
        (inner.player.state(), inner.player.queue_state())
    }

    /// Serializes auth transitions and cookie cleanup with playback commands.
    pub(crate) async fn lock_for_session_change(&self) -> SessionPlayerGuard<'_> {
        SessionPlayerGuard {
            inner: self.inner.lock().await,
        }
    }

    /// Stops playback and keeps player commands serialized until sign-out
    /// finishes, so they cannot reopen the window during cookie cleanup.
    pub(crate) async fn stop_for_sign_out<T>(
        &self,
        app: &AppHandle,
        sign_out: impl Future<Output = T>,
    ) -> T {
        let mut player = self.lock_for_session_change().await;
        player.stop(app);
        sign_out.await
    }
}

fn start_autoplay(app: &AppHandle, inner: &mut Inner) {
    let state = app.state::<AppState>();
    let epoch = state.session.epoch();
    let Some(seed) = inner.autoplay.start(|| {
        if state.session.status() == AuthStatus::SignedIn {
            inner.player.needs_more()
        } else {
            None
        }
    }) else {
        return;
    };
    let ticket = Ticket {
        generation: inner.player.radio_generation(),
        epoch,
    };
    tauri::async_runtime::spawn(fetch_autoplay(app.clone(), seed, ticket));
}

async fn fetch_autoplay(app: AppHandle, seed: Seed, ticket: Ticket) {
    let state = app.state::<AppState>();
    let result = fetch_radio(&state, seed, ticket.epoch).await;
    if state.session.epoch() != ticket.epoch {
        tracing::debug!("discarding autoplay fetch after session change");
    } else if let Err(error) = &result {
        tracing::warn!(%error, "autoplay fetch failed");
        on_error(&app, ticket.epoch, error.clone()).await;
    }

    let mut inner = state.player.inner.lock().await;
    let current = Ticket {
        generation: inner.player.radio_generation(),
        epoch: state.session.epoch(),
    };
    let mut notification = None;
    if inner.autoplay.finish(ticket, current)
        && let Ok(mut radio) = result
    {
        radio
            .songs
            .retain(|song| VideoId::parse(&song.video_id).is_some());
        let now = Instant::now();
        let outcome = inner.player.append(radio.songs, now);
        let appended = outcome.queue.is_some();
        inner.player.set_radio_continuation(radio.continuation);
        // A slow response may arrive after the current track already ended.
        // Publish the append first, then advance using the normal load path.
        let (mut applied, notice) = inner.apply(&app, outcome).await;
        notification = notice;
        if applied.is_ok() && appended && inner.player.is_ended() {
            let now = Instant::now();
            let next = inner.player.next(now);
            if next.queue.is_some() {
                inner.autoplay.request();
            }
            (applied, _) = inner.apply(&app, next).await;
        }
        if let Err(error) = applied {
            tracing::warn!(%error, "could not apply autoplay queue");
        }
    }
    start_autoplay(&app, &mut inner);
    drop(inner);
    send_track_notification(&app, notification);
}

async fn fetch_radio(state: &AppState, seed: Seed, epoch: u64) -> Result<RadioQueue, AppError> {
    if !state.launch.wait(LAUNCH_REFRESH_TIMEOUT).await {
        tracing::warn!("launch cookie refresh did not finish in time");
    }
    let client = state.client().await?;
    if state.session.epoch() != epoch || state.session.status() != AuthStatus::SignedIn {
        return Err(AppError::not_signed_in());
    }
    Ok(match seed {
        Seed::Video(id) => parse_radio(&client.radio(id.as_str()).await?)?,
        Seed::Continuation(token) => {
            parse_radio_continuation(&client.radio_continuation(&token).await?)?
        }
    })
}

impl Default for PlayerService {
    fn default() -> Self {
        Self::new()
    }
}

struct TrackNotification {
    title: String,
    body: String,
}

fn track_notification(last: &mut Option<String>, state: &PlayerState) -> Option<TrackNotification> {
    if state.status == PlaybackStatus::Idle {
        *last = None;
    }
    if state.status != PlaybackStatus::Playing || state.is_ad {
        return None;
    }
    let id = &state.track.as_ref()?.video_id;
    if last.as_ref() == Some(id) {
        return None;
    }
    let metadata = media::now_playing(state)?;
    *last = Some(id.clone());
    Some(TrackNotification {
        title: metadata.title,
        body: metadata.artist,
    })
}

fn notify_if_enabled(
    notification: Option<TrackNotification>,
    enabled: bool,
    send: impl FnOnce(TrackNotification),
) {
    if enabled && let Some(notification) = notification {
        send(notification);
    }
}

fn send_track_notification(app: &AppHandle, notification: Option<TrackNotification>) {
    if notification.is_none() {
        return;
    }
    notify_if_enabled(
        notification,
        app.state::<TrackNotifications>().enabled(),
        |notification| {
            if let Err(error) = app
                .notification()
                .builder()
                .title(notification.title)
                .body(notification.body)
                .show()
            {
                tracing::debug!(%error, "could not show track notification");
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ytm_core::models::{ArtistRef, Song};

    fn track(video_id: &str, artists: &[&str]) -> Song {
        Song {
            album: None,
            is_podcast_episode: None,
            video_id: video_id.into(),
            title: "Test song".into(),
            artists: artists
                .iter()
                .map(|name| ArtistRef {
                    name: (*name).into(),
                    browse_id: None,
                })
                .collect(),
            thumbnail_url: None,
        }
    }

    #[test]
    fn task13_track_notification_gate_obeys_the_setting() {
        let mut state = Player::new(1).state();
        state.status = PlaybackStatus::Playing;
        state.track = Some(track("test-vid-0A", &[]));

        for enabled in [false, true] {
            let mut sent = false;
            let notice = track_notification(&mut None, &state);
            notify_if_enabled(notice, enabled, |_| sent = true);
            assert_eq!(sent, enabled);
        }
    }

    #[test]
    fn track_notification_waits_for_non_ad_playback_and_deduplicates() {
        let mut state = Player::new(1).state();
        state.track = Some(track("test-vid-0A", &["Artist A", "Artist B"]));
        let mut last = None;

        state.status = PlaybackStatus::Loading;
        assert!(track_notification(&mut last, &state).is_none());
        state.status = PlaybackStatus::Playing;
        state.is_ad = true;
        assert!(track_notification(&mut last, &state).is_none());

        state.is_ad = false;
        let notice = track_notification(&mut last, &state).unwrap();
        assert_eq!(notice.title, "Test song");
        assert_eq!(notice.body, "Artist A, Artist B");
        state.status = PlaybackStatus::Paused;
        assert!(track_notification(&mut last, &state).is_none());
        state.status = PlaybackStatus::Playing;
        assert!(track_notification(&mut last, &state).is_none());

        state.track = Some(track("test-vid-0B", &[]));
        assert_eq!(track_notification(&mut last, &state).unwrap().body, "");
    }

    #[test]
    fn track_notification_can_repeat_the_same_id_after_stop() {
        let mut state = Player::new(1).state();
        let mut last = Some("test-vid-0A".into());
        assert!(track_notification(&mut last, &state).is_none());
        assert_eq!(last, None);

        state.status = PlaybackStatus::Playing;
        state.track = Some(track("test-vid-0A", &[]));
        assert!(track_notification(&mut last, &state).is_some());
    }
}
