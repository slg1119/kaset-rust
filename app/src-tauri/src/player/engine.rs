//! Player state machine. Pure Rust: commands and player-page events go in,
//! an `Outcome` (what to tell the player page, what to emit) comes out. The
//! Tauri glue in `service.rs` applies outcomes.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ytm_core::models::Song;

use super::queue::{Advance, Queue, RepeatMode};
use super::video_id::VideoId;

/// `player://state` is emitted at most this often for progress-only changes.
pub const STATE_INTERVAL: Duration = Duration::from_millis(250);
/// Previous restarts the current song when it has played longer than this.
pub const PREVIOUS_RESTART_SECONDS: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackStatus {
    /// Nothing queued.
    Idle,
    /// The player page is loading the current song.
    Loading,
    Playing,
    Paused,
    /// The last song finished and nothing follows it.
    Ended,
}

/// Emitted on `player://state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub track: Option<Song>,
    /// Seconds.
    pub position: f64,
    /// Seconds; 0 while unknown.
    pub duration: f64,
    /// An ad is playing; position and duration belong to the ad.
    pub is_ad: bool,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub has_previous: bool,
    pub has_next: bool,
}

/// Emitted on `player://queue`: songs in play order and the current index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct QueueState {
    pub songs: Vec<Song>,
    pub current: Option<u32>,
}

/// Sent by the init script in the player window through `playback_event`.
/// Fixed-size fields only: no free-form strings, lists, or maps.
#[derive(Debug, Clone, PartialEq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaybackEvent {
    /// Sampled on media events and about four times a second while playing.
    Progress {
        video_id: VideoId,
        playing: bool,
        position: f64,
        duration: f64,
        ad: bool,
    },
    /// The song's media ended (never sent for an ad).
    Ended { video_id: VideoId },
    /// The page could not start or continue playback.
    Failed { video_id: VideoId },
}

impl PlaybackEvent {
    pub fn video_id(&self) -> &VideoId {
        match self {
            Self::Progress { video_id, .. }
            | Self::Ended { video_id }
            | Self::Failed { video_id } => video_id,
        }
    }
}

/// What the player page should do.
#[derive(Debug, Clone, PartialEq)]
pub enum WebCommand {
    /// Navigate the player window to the song's watch page; it starts playing.
    Load(VideoId),
    Play,
    Pause,
    /// Seconds.
    Seek(f64),
    /// Close the player window.
    Stop,
}

/// The result of one command or event.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub command: Option<WebCommand>,
    /// Emit on `player://state` (already throttled).
    pub state: Option<PlayerState>,
    /// Emit on `player://queue`.
    pub queue: Option<QueueState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PlayerError {
    #[error("nothing playable at that position")]
    NothingToPlay,
}

fn seconds(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub struct Player {
    queue: Queue,
    status: PlaybackStatus,
    position: f64,
    duration: f64,
    is_ad: bool,
    last_emit: Option<Instant>,
}

impl Player {
    pub fn new(seed: u64) -> Self {
        Self {
            queue: Queue::new(seed),
            status: PlaybackStatus::Idle,
            position: 0.0,
            duration: 0.0,
            is_ad: false,
            last_emit: None,
        }
    }

    pub fn state(&self) -> PlayerState {
        PlayerState {
            status: self.status,
            track: self.queue.current().cloned(),
            position: self.position,
            duration: self.duration,
            is_ad: self.is_ad,
            repeat: self.queue.repeat(),
            shuffle: self.queue.shuffle(),
            has_previous: self.queue.has_previous(),
            has_next: self.queue.has_next(),
        }
    }

    pub fn queue_state(&self) -> QueueState {
        QueueState {
            songs: self.queue.ordered(),
            current: self
                .queue
                .position()
                .and_then(|position| u32::try_from(position).ok()),
        }
    }

    /// The song the player page should be playing.
    pub fn current_video_id(&self) -> Option<VideoId> {
        self.queue
            .current()
            .and_then(|song| VideoId::parse(&song.video_id))
    }

    /// State to emit: always when `force`, otherwise at most every `STATE_INTERVAL`.
    fn emit(&mut self, now: Instant, force: bool) -> Option<PlayerState> {
        let due = self
            .last_emit
            .is_none_or(|last| now.saturating_duration_since(last) >= STATE_INTERVAL);
        (force || due).then(|| {
            self.last_emit = Some(now);
            self.state()
        })
    }

    /// Starts loading the current song.
    fn load_current(&mut self, now: Instant) -> Outcome {
        let Some(id) = self.current_video_id() else {
            return self.stop(now);
        };
        self.status = PlaybackStatus::Loading;
        self.position = 0.0;
        self.duration = 0.0;
        self.is_ad = false;
        Outcome {
            command: Some(WebCommand::Load(id)),
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Replaces the queue with `songs` and plays `songs[start]`. Songs whose
    /// video id is invalid are dropped.
    pub fn play_songs(
        &mut self,
        songs: Vec<Song>,
        start: usize,
        now: Instant,
    ) -> Result<Outcome, PlayerError> {
        let start_valid = songs
            .get(start)
            .is_some_and(|song| VideoId::parse(&song.video_id).is_some());
        if !start_valid {
            return Err(PlayerError::NothingToPlay);
        }
        let skipped_before = songs[..start]
            .iter()
            .filter(|song| VideoId::parse(&song.video_id).is_none())
            .count();
        let playable: Vec<Song> = songs
            .into_iter()
            .filter(|song| VideoId::parse(&song.video_id).is_some())
            .collect();
        self.queue.replace(playable, start - skipped_before);
        Ok(self.load_current(now))
    }

    pub fn play(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Idle | PlaybackStatus::Playing => Outcome::default(),
            // The page holds playback after the last song; start it again.
            PlaybackStatus::Ended => self.load_current(now),
            PlaybackStatus::Loading | PlaybackStatus::Paused => {
                self.status = PlaybackStatus::Playing;
                Outcome {
                    command: Some(WebCommand::Play),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
        }
    }

    pub fn pause(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Loading | PlaybackStatus::Playing => {
                self.status = PlaybackStatus::Paused;
                Outcome {
                    command: Some(WebCommand::Pause),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
            _ => Outcome::default(),
        }
    }

    pub fn toggle(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Loading | PlaybackStatus::Playing => self.pause(now),
            _ => self.play(now),
        }
    }

    pub fn next(&mut self, now: Instant) -> Outcome {
        if self.queue.advance(Advance::User).is_some() {
            self.load_current(now)
        } else {
            Outcome::default()
        }
    }

    /// Restarts the song after `PREVIOUS_RESTART_SECONDS` or at the start of
    /// the queue; otherwise moves to the previous song.
    pub fn previous(&mut self, now: Instant) -> Outcome {
        if self.queue.current().is_none() {
            return Outcome::default();
        }
        if self.position > PREVIOUS_RESTART_SECONDS || !self.queue.has_previous() {
            return self.seek(0.0, now);
        }
        self.queue.retreat();
        self.load_current(now)
    }

    /// Ignored while an ad plays.
    pub fn seek(&mut self, position: f64, now: Instant) -> Outcome {
        if self.queue.current().is_none() || self.is_ad {
            return Outcome::default();
        }
        let mut position = seconds(position);
        if self.duration > 0.0 {
            position = position.min(self.duration);
        }
        self.position = position;
        Outcome {
            command: Some(WebCommand::Seek(position)),
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode, now: Instant) -> Outcome {
        self.queue.set_repeat(repeat);
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_shuffle(&mut self, shuffle: bool, now: Instant) -> Outcome {
        self.queue.set_shuffle(shuffle);
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Empties the queue and closes the player window (sign-out).
    pub fn stop(&mut self, now: Instant) -> Outcome {
        self.queue.clear();
        self.status = PlaybackStatus::Idle;
        self.position = 0.0;
        self.duration = 0.0;
        self.is_ad = false;
        Outcome {
            command: Some(WebCommand::Stop),
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    /// Applies an event from the player page. Events for any video other than
    /// the current song are ignored (the page's own autoplay, a late event
    /// from the previous song, or a forged event).
    pub fn handle_event(&mut self, event: PlaybackEvent, now: Instant) -> Outcome {
        if self.current_video_id().as_ref() != Some(event.video_id()) {
            return Outcome::default();
        }
        match event {
            PlaybackEvent::Progress {
                playing,
                position,
                duration,
                ad,
                ..
            } => {
                let status = match (self.status, playing) {
                    (_, true) => PlaybackStatus::Playing,
                    // Still waiting for the page to start the song.
                    (PlaybackStatus::Loading, false) => PlaybackStatus::Loading,
                    (PlaybackStatus::Ended, false) => PlaybackStatus::Ended,
                    (_, false) => PlaybackStatus::Paused,
                };
                let force = status != self.status || ad != self.is_ad;
                self.status = status;
                self.position = seconds(position);
                self.duration = seconds(duration);
                self.is_ad = ad;
                Outcome {
                    command: None,
                    state: self.emit(now, force),
                    queue: None,
                }
            }
            PlaybackEvent::Ended { .. } => {
                // A duplicate ended for a song already advanced from, or an ad.
                if matches!(self.status, PlaybackStatus::Loading | PlaybackStatus::Ended)
                    || self.is_ad
                {
                    return Outcome::default();
                }
                if self.queue.advance(Advance::Ended).is_some() {
                    return self.load_current(now);
                }
                self.status = PlaybackStatus::Ended;
                self.position = self.duration;
                Outcome {
                    command: Some(WebCommand::Pause),
                    state: self.emit(now, true),
                    queue: None,
                }
            }
            PlaybackEvent::Failed { .. } => {
                if self.status == PlaybackStatus::Ended {
                    return Outcome::default();
                }
                self.status = PlaybackStatus::Paused;
                Outcome {
                    command: None,
                    state: self.emit(now, true),
                    queue: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "test-vid-0A";
    const B: &str = "test-vid-0B";
    const C: &str = "test-vid-0C";

    fn song(id: &str) -> Song {
        Song {
            video_id: id.into(),
            title: format!("Song {id}"),
            artists: Vec::new(),
            thumbnail_url: None,
        }
    }

    fn id(value: &str) -> VideoId {
        VideoId::parse(value).unwrap()
    }

    fn progress(video: &str, playing: bool, position: f64) -> PlaybackEvent {
        PlaybackEvent::Progress {
            video_id: id(video),
            playing,
            position,
            duration: 200.0,
            ad: false,
        }
    }

    fn ended(video: &str) -> PlaybackEvent {
        PlaybackEvent::Ended {
            video_id: id(video),
        }
    }

    /// A player playing `songs[start]`, with the throttle window passed.
    fn playing(ids: &[&str], start: usize) -> (Player, Instant) {
        let mut player = Player::new(1);
        let now = Instant::now();
        player
            .play_songs(ids.iter().map(|value| song(value)).collect(), start, now)
            .unwrap();
        player.handle_event(progress(ids[start], true, 0.0), now);
        (player, now + STATE_INTERVAL)
    }

    #[test]
    fn play_songs_loads_the_start_song() {
        let mut player = Player::new(1);
        let outcome = player
            .play_songs(vec![song(A), song(B)], 1, Instant::now())
            .unwrap();
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        let state = outcome.state.unwrap();
        assert_eq!(state.status, PlaybackStatus::Loading);
        assert_eq!(state.track, Some(song(B)));
        assert_eq!(outcome.queue.unwrap().current, Some(1));
    }

    #[test]
    fn play_songs_drops_invalid_ids_and_keeps_the_start_song() {
        let mut player = Player::new(1);
        let outcome = player
            .play_songs(
                vec![song("bad id"), song(A), song("x"), song(B)],
                3,
                Instant::now(),
            )
            .unwrap();
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        let queue = outcome.queue.unwrap();
        assert_eq!(queue.songs, vec![song(A), song(B)]);
        assert_eq!(queue.current, Some(1));
    }

    #[test]
    fn play_songs_rejects_an_invalid_start() {
        let mut player = Player::new(1);
        let now = Instant::now();
        assert_eq!(
            player.play_songs(vec![song(A)], 1, now),
            Err(PlayerError::NothingToPlay)
        );
        assert_eq!(
            player.play_songs(vec![song("javascript:")], 0, now),
            Err(PlayerError::NothingToPlay)
        );
        assert_eq!(player.state().status, PlaybackStatus::Idle);
    }

    #[test]
    fn progress_for_another_video_is_ignored() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.handle_event(progress(C, false, 50.0), now);
        assert_eq!(outcome, Outcome::default());
        assert_eq!(player.state().status, PlaybackStatus::Playing);
        assert_eq!(player.state().position, 0.0);
    }

    #[test]
    fn loading_stays_loading_until_the_page_plays() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player.play_songs(vec![song(A)], 0, now).unwrap();
        player.handle_event(progress(A, false, 0.0), now + STATE_INTERVAL);
        assert_eq!(player.state().status, PlaybackStatus::Loading);
        let outcome = player.handle_event(progress(A, true, 0.1), now + STATE_INTERVAL);
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Playing);
    }

    #[test]
    fn progress_is_throttled_but_status_changes_are_not() {
        let (mut player, now) = playing(&[A], 0);
        assert!(
            player
                .handle_event(progress(A, true, 1.0), now)
                .state
                .is_some()
        );
        let soon = now + Duration::from_millis(100);
        assert!(
            player
                .handle_event(progress(A, true, 1.1), soon)
                .state
                .is_none()
        );
        assert_eq!(player.state().position, 1.1, "state still updates");
        let paused = player.handle_event(progress(A, false, 1.2), soon);
        assert_eq!(paused.state.unwrap().status, PlaybackStatus::Paused);
        let later = soon + STATE_INTERVAL;
        assert!(
            player
                .handle_event(progress(A, false, 1.2), later)
                .state
                .is_some()
        );
    }

    #[test]
    fn ended_loads_the_next_song_once() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(B))));
        assert_eq!(outcome.queue.unwrap().current, Some(1));
        // A late duplicate for A, and a duplicate for B while it loads.
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
        assert_eq!(player.handle_event(ended(B), now), Outcome::default());
    }

    #[test]
    fn ended_on_the_last_song_holds_the_page() {
        let (mut player, now) = playing(&[A], 0);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Pause));
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Ended);
        // Paused progress after the end keeps the ended state.
        player.handle_event(progress(A, false, 200.0), now + STATE_INTERVAL);
        assert_eq!(player.state().status, PlaybackStatus::Ended);
        // Play starts the song again.
        let again = player.play(now + STATE_INTERVAL);
        assert_eq!(again.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_with_repeat_one_reloads_the_same_song() {
        let (mut player, now) = playing(&[A, B], 0);
        player.set_repeat(RepeatMode::One, now);
        let outcome = player.handle_event(ended(A), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_with_repeat_all_wraps() {
        let (mut player, now) = playing(&[A, B], 1);
        player.set_repeat(RepeatMode::All, now);
        let outcome = player.handle_event(ended(B), now);
        assert_eq!(outcome.command, Some(WebCommand::Load(id(A))));
    }

    #[test]
    fn ended_during_an_ad_is_ignored() {
        let (mut player, now) = playing(&[A, B], 0);
        player.handle_event(
            PlaybackEvent::Progress {
                video_id: id(A),
                playing: true,
                position: 3.0,
                duration: 15.0,
                ad: true,
            },
            now,
        );
        assert!(player.state().is_ad);
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
        assert_eq!(
            player.seek(10.0, now),
            Outcome::default(),
            "no seeking during ads"
        );
    }

    #[test]
    fn toggle_pauses_and_resumes() {
        let (mut player, now) = playing(&[A], 0);
        let paused = player.toggle(now);
        assert_eq!(paused.command, Some(WebCommand::Pause));
        assert_eq!(player.state().status, PlaybackStatus::Paused);
        let resumed = player.toggle(now);
        assert_eq!(resumed.command, Some(WebCommand::Play));
        assert_eq!(player.state().status, PlaybackStatus::Playing);
    }

    #[test]
    fn controls_do_nothing_when_idle() {
        let mut player = Player::new(1);
        let now = Instant::now();
        assert_eq!(player.toggle(now), Outcome::default());
        assert_eq!(player.next(now), Outcome::default());
        assert_eq!(player.previous(now), Outcome::default());
        assert_eq!(player.seek(5.0, now), Outcome::default());
    }

    #[test]
    fn next_moves_and_stops_at_the_end() {
        let (mut player, now) = playing(&[A, B], 0);
        assert_eq!(player.next(now).command, Some(WebCommand::Load(id(B))));
        player.handle_event(progress(B, true, 0.0), now);
        assert_eq!(player.next(now), Outcome::default());
    }

    #[test]
    fn previous_restarts_after_three_seconds_and_otherwise_goes_back() {
        let (mut player, now) = playing(&[A, B], 1);
        player.handle_event(progress(B, true, 10.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Seek(0.0)));
        player.handle_event(progress(B, true, 1.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Load(id(A))));
        // At the start of the queue previous restarts.
        player.handle_event(progress(A, true, 1.0), now);
        assert_eq!(player.previous(now).command, Some(WebCommand::Seek(0.0)));
    }

    #[test]
    fn seek_clamps_to_the_duration() {
        let (mut player, now) = playing(&[A], 0);
        player.handle_event(progress(A, true, 1.0), now);
        assert_eq!(
            player.seek(500.0, now).command,
            Some(WebCommand::Seek(200.0))
        );
        assert_eq!(player.seek(-3.0, now).command, Some(WebCommand::Seek(0.0)));
        assert_eq!(
            player.seek(f64::NAN, now).command,
            Some(WebCommand::Seek(0.0))
        );
    }

    #[test]
    fn non_finite_progress_is_sanitized() {
        let (mut player, now) = playing(&[A], 0);
        player.handle_event(
            PlaybackEvent::Progress {
                video_id: id(A),
                playing: true,
                position: f64::INFINITY,
                duration: f64::NAN,
                ad: false,
            },
            now,
        );
        assert_eq!(
            (player.state().position, player.state().duration),
            (0.0, 0.0)
        );
    }

    #[test]
    fn shuffle_emits_the_new_queue_order() {
        let (mut player, now) = playing(&[A, B, C], 0);
        let outcome = player.set_shuffle(true, now);
        let queue = outcome.queue.unwrap();
        assert_eq!(queue.current, Some(0));
        assert_eq!(queue.songs[0], song(A));
        assert!(outcome.state.unwrap().shuffle);
    }

    #[test]
    fn failed_pauses() {
        let (mut player, now) = playing(&[A], 0);
        let outcome = player.handle_event(PlaybackEvent::Failed { video_id: id(A) }, now);
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Paused);
    }

    #[test]
    fn stop_empties_the_queue_and_closes_the_page() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.stop(now);
        assert_eq!(outcome.command, Some(WebCommand::Stop));
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Idle);
        assert_eq!(outcome.queue.unwrap().songs, Vec::<Song>::new());
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
    }

    #[test]
    fn events_deserialize_from_the_init_script_shape() {
        let event: PlaybackEvent = serde_json::from_str(
            r#"{"kind":"progress","video_id":"test-vid-0A","playing":true,"position":1.5,"duration":200,"ad":false}"#,
        )
        .unwrap();
        assert_eq!(event, progress(A, true, 1.5));
        let event: PlaybackEvent =
            serde_json::from_str(r#"{"kind":"ended","video_id":"test-vid-0A"}"#).unwrap();
        assert_eq!(event, ended(A));
    }

    #[test]
    fn events_with_bad_ids_or_extra_fields_are_rejected() {
        for json in [
            r#"{"kind":"ended","video_id":"not-a-valid-id"}"#,
            r#"{"kind":"ended","video_id":"test-vid-0A","url":"https://example.com"}"#,
            r#"{"kind":"navigate","video_id":"test-vid-0A"}"#,
            r#"{"kind":"progress","video_id":"test-vid-0A","playing":"yes","position":1,"duration":2,"ad":false}"#,
        ] {
            assert!(
                serde_json::from_str::<PlaybackEvent>(json).is_err(),
                "{json}"
            );
        }
    }
}
