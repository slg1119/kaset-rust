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
const MAX_MEDIA_SECONDS: f64 = 86_400.0 * 7.0;

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
    pub browse_track: Option<BrowseTrack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct BrowseTrack {
    pub browse_id: String,
    pub index: u32,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct BrowsePlayback {
    pub browse_id: String,
    pub offset: u32,
    pub shuffle: bool,
}

struct BrowseQueue {
    browse_id: String,
    indices: Vec<u32>,
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

#[derive(Debug, PartialEq, Eq)]
pub enum Seed {
    Video(VideoId),
    Continuation(String),
}

fn seconds(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, MAX_MEDIA_SECONDS)
    } else {
        0.0
    }
}

fn position_seconds(position: f64, duration: f64) -> f64 {
    let position = seconds(position);
    if duration > 0.0 {
        position.min(duration)
    } else {
        position
    }
}

pub struct Player {
    queue: Queue,
    queue_generation: u64,
    browse_queue: Option<BrowseQueue>,
    radio_continuation: Option<String>,
    radio_seed: Option<VideoId>,
    radio_generation: u64,
    autoplay_suspended: bool,
    autoplay_revision: u64,
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
            queue_generation: 0,
            browse_queue: None,
            radio_continuation: None,
            radio_seed: None,
            radio_generation: 0,
            autoplay_suspended: false,
            autoplay_revision: 0,
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
            browse_track: self.browse_queue.as_ref().and_then(|origin| {
                let index = *origin.indices.get(self.queue.original_position()?)?;
                Some(BrowseTrack {
                    browse_id: origin.browse_id.clone(),
                    index,
                })
            }),
        }
    }

    pub fn queue_position(&self) -> (u64, Option<usize>) {
        (self.queue_generation, self.queue.position())
    }

    pub(super) fn radio_generation(&self) -> u64 {
        self.radio_generation
    }

    pub(super) fn autoplay_trigger(&self) -> (u64, Option<usize>, u64) {
        (
            self.radio_generation,
            self.queue.position(),
            self.autoplay_revision,
        )
    }

    fn retry_autoplay(&mut self) {
        self.autoplay_suspended = false;
        self.autoplay_revision += 1;
    }

    fn invalidate_radio_tail(&mut self) {
        self.queue_generation += 1;
        self.radio_generation += 1;
        self.radio_continuation = None;
        self.autoplay_suspended = false;
    }

    fn reset_radio(&mut self) {
        self.invalidate_radio_tail();
        self.radio_seed = None;
    }

    pub fn needs_more(&self) -> Option<Seed> {
        if self.status == PlaybackStatus::Idle || self.autoplay_suspended {
            return None;
        }
        let remaining = self.queue.remaining()?;
        if let Some(seed) = &self.radio_seed {
            return Some(Seed::Video(seed.clone()));
        }
        if self.queue.repeat() != RepeatMode::Off || remaining >= 3 {
            return None;
        }
        match &self.radio_continuation {
            Some(token) => Some(Seed::Continuation(token.clone())),
            None => VideoId::parse(&self.queue.last()?.video_id).map(Seed::Video),
        }
    }

    pub fn set_radio_continuation(&mut self, continuation: Option<String>) {
        self.radio_seed = None;
        self.radio_continuation = continuation;
    }

    pub fn append(&mut self, songs: Vec<Song>, now: Instant) -> Outcome {
        if self.queue.append(songs) == 0 {
            return Outcome::default();
        }
        self.publish_queue(now)
    }

    pub fn is_ended(&self) -> bool {
        self.status == PlaybackStatus::Ended
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
        if self.status == PlaybackStatus::Idle {
            self.retry_autoplay();
        }
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
        self.replace_queue(songs, start, None, now)
    }

    pub fn play_browse_songs(
        &mut self,
        songs: Vec<Song>,
        start: usize,
        playback: BrowsePlayback,
        now: Instant,
    ) -> Result<Outcome, PlayerError> {
        self.replace_queue(songs, start, Some(playback), now)
    }

    fn replace_queue(
        &mut self,
        songs: Vec<Song>,
        start: usize,
        playback: Option<BrowsePlayback>,
        now: Instant,
    ) -> Result<Outcome, PlayerError> {
        if !songs
            .get(start)
            .is_some_and(|song| VideoId::parse(&song.video_id).is_some())
        {
            return Err(PlayerError::NothingToPlay);
        }
        let (indices, playable): (Vec<_>, Vec<_>) = songs
            .into_iter()
            .enumerate()
            .filter(|(_, song)| VideoId::parse(&song.video_id).is_some())
            .unzip();
        let start = indices.partition_point(|&index| index < start);
        let shuffle = playback.as_ref().is_some_and(|origin| origin.shuffle);
        self.browse_queue = playback.and_then(|origin| {
            let indices = indices
                .iter()
                .map(|&index| u32::try_from(index).ok()?.checked_add(origin.offset))
                .collect::<Option<Vec<_>>>()?;
            Some(BrowseQueue {
                browse_id: origin.browse_id,
                indices,
            })
        });
        self.queue.replace(playable, start);
        if shuffle {
            self.queue.set_shuffle(true);
        }
        self.reset_radio();
        Ok(self.load_current(now))
    }

    pub fn start_radio(&mut self, song: Song, now: Instant) -> Result<Outcome, PlayerError> {
        let outcome = self.play_songs(vec![song], 0, now)?;
        self.radio_seed = self.current_video_id();
        Ok(outcome)
    }

    pub fn insert_next(&mut self, songs: Vec<Song>, now: Instant) -> Outcome {
        self.add_songs(songs, true, now)
    }

    pub fn enqueue(&mut self, songs: Vec<Song>, now: Instant) -> Outcome {
        self.add_songs(songs, false, now)
    }

    fn add_songs(&mut self, mut songs: Vec<Song>, next: bool, now: Instant) -> Outcome {
        songs.retain(|song| VideoId::parse(&song.video_id).is_some());
        if self.queue.current().is_none() {
            let mut seen = std::collections::HashSet::new();
            songs.retain(|song| seen.insert(song.video_id.clone()));
            songs.truncate(super::queue::MAX_QUEUE_LEN);
            if songs.is_empty() {
                return Outcome::default();
            }
            self.queue.replace(songs, 0);
            self.status = PlaybackStatus::Idle;
            self.browse_queue = None;
            self.reset_radio();
            return self.publish_queue(now);
        }
        let changed = if next {
            self.queue.insert_next(songs)
        } else {
            self.queue.enqueue(songs) > 0
        };
        self.queue_edited(changed, now)
    }

    pub fn remove(&mut self, index: usize, now: Instant) -> Outcome {
        let was_current = self.queue.position() == Some(index);
        if !self.queue.remove(index) {
            return Outcome::default();
        }
        self.record_queue_edit();
        if was_current {
            if self.queue.current().is_none() {
                self.status = PlaybackStatus::Ended;
                self.position = 0.0;
                self.duration = 0.0;
                self.is_ad = false;
                let mut outcome = self.publish_queue(now);
                outcome.command = Some(WebCommand::Stop);
                return outcome;
            }
            if self.status != PlaybackStatus::Idle {
                return self.load_current(now);
            }
        }
        self.publish_queue(now)
    }

    pub fn move_item(&mut self, from: usize, to: usize, now: Instant) -> Outcome {
        let changed = self.queue.move_item(from, to);
        self.queue_edited(changed, now)
    }

    pub fn jump_to(&mut self, index: usize, now: Instant) -> Outcome {
        if self.queue.jump_to(index) {
            self.load_current(now)
        } else {
            Outcome::default()
        }
    }

    pub fn clear_upcoming(&mut self, now: Instant) -> Outcome {
        let changed = self.queue.clear_upcoming();
        self.browse_queue = None;
        // Invalidate even when the queue already has no upcoming songs.
        self.reset_radio();
        self.autoplay_suspended = true;
        if changed {
            self.publish_queue(now)
        } else {
            Outcome::default()
        }
    }

    fn queue_edited(&mut self, changed: bool, now: Instant) -> Outcome {
        if !changed {
            return Outcome::default();
        }
        self.record_queue_edit();
        self.publish_queue(now)
    }

    fn record_queue_edit(&mut self) {
        // Edits reindex the detail-row mapping and supersede pending page playback.
        // The radio continuation still belongs to the same radio session.
        self.browse_queue = None;
        self.queue_generation += 1;
        self.retry_autoplay();
    }

    fn publish_queue(&mut self, now: Instant) -> Outcome {
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: Some(self.queue_state()),
        }
    }

    pub fn play(&mut self, now: Instant) -> Outcome {
        match self.status {
            PlaybackStatus::Playing => Outcome::default(),
            PlaybackStatus::Idle | PlaybackStatus::Ended if self.queue.current().is_none() => {
                Outcome::default()
            }
            PlaybackStatus::Idle => self.load_current(now),
            // The page holds playback after the last song; start it again.
            PlaybackStatus::Ended => {
                self.retry_autoplay();
                self.load_current(now)
            }
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
        if self.is_ended() {
            self.retry_autoplay();
        }
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
        if self.status == PlaybackStatus::Idle || self.queue.current().is_none() || self.is_ad {
            return Outcome::default();
        }
        let position = position_seconds(position, self.duration);
        self.position = position;
        Outcome {
            command: Some(WebCommand::Seek(position)),
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode, now: Instant) -> Outcome {
        if repeat == RepeatMode::Off && self.queue.repeat() != repeat {
            self.autoplay_revision += 1;
        }
        self.queue.set_repeat(repeat);
        Outcome {
            command: None,
            state: self.emit(now, true),
            queue: None,
        }
    }

    pub fn set_shuffle(&mut self, shuffle: bool, now: Instant) -> Outcome {
        if shuffle != self.queue.shuffle() {
            let tail = self.queue.last().map(|song| song.video_id.clone());
            self.queue.set_shuffle(shuffle);
            if tail.as_ref() != self.queue.last().map(|song| &song.video_id) {
                self.queue_generation += 1;
                self.radio_generation += 1;
                self.radio_continuation = None;
            }
            self.autoplay_revision += 1;
        }
        self.publish_queue(now)
    }

    /// Empties the queue and closes the player window.
    pub fn stop(&mut self, now: Instant) -> Outcome {
        self.queue.clear();
        self.browse_queue = None;
        self.reset_radio();
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
                let force = status != self.status;
                self.status = status;
                self.duration = seconds(duration);
                self.position = position_seconds(position, self.duration);
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
                let force = self.status != PlaybackStatus::Paused;
                self.status = PlaybackStatus::Paused;
                Outcome {
                    command: None,
                    state: self.emit(now, force),
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
    fn queue_review_play_next_replays_current_and_played_songs() {
        let (mut player, now) = playing(&[A, B, C], 1);
        let edits = player.insert_next(vec![song(B), song(A), song(C), song(B)], now);
        assert_eq!(edits.command, None);
        assert_eq!(
            edits.queue.unwrap().songs,
            vec![song(A), song(B), song(B), song(A), song(C)]
        );
        assert_eq!(player.queue_state().current, Some(1));
        assert_eq!(player.insert_next(vec![song(B)], now), Outcome::default());
        assert!(player.insert_next(vec![song(C)], now).queue.is_some());
        assert_eq!(player.queue_state().songs[2], song(C));
    }

    #[test]
    fn queue_review_enqueue_replays_current_and_played_songs_but_skips_upcoming() {
        let (mut player, now) = playing(&[A, B, C], 1);
        let edits = player.enqueue(vec![song(B), song(A), song(C), song(A)], now);
        assert_eq!(edits.command, None);
        assert_eq!(
            edits.queue.unwrap().songs,
            vec![song(A), song(B), song(C), song(B), song(A)]
        );
        assert_eq!(player.queue_state().current, Some(1));
        assert_eq!(
            player.enqueue(vec![song(C), song(B)], now),
            Outcome::default()
        );
    }

    #[test]
    fn queue_review_removing_current_in_the_middle_loads_the_next_song() {
        let (mut player, now) = playing(&[A, B, C], 1);
        let edits = player.remove(1, now);
        assert_eq!(edits.command, Some(WebCommand::Load(id(C))));
        let queue = edits.queue.unwrap();
        assert_eq!(queue.songs, vec![song(A), song(C)]);
        assert_eq!(queue.current, Some(1));
    }

    #[test]
    fn queue_review_removing_current_at_the_end_stops_and_keeps_history() {
        for (tracks, current) in [(&[A, B][..], 1), (&[A][..], 0)] {
            let (mut player, now) = playing(tracks, current);
            let edits = player.remove(current, now);
            assert_eq!(edits.command, Some(WebCommand::Stop));
            let state = edits.state.unwrap();
            assert_eq!(state.status, PlaybackStatus::Ended);
            assert_eq!(state.track, None);
            let queue = edits.queue.unwrap();
            assert_eq!(
                queue.songs,
                tracks[..current]
                    .iter()
                    .map(|value| song(value))
                    .collect::<Vec<_>>()
            );
            assert_eq!(queue.current, None);
            assert_eq!(player.play(now), Outcome::default());
        }
    }

    #[test]
    fn queue_review_removing_played_song_decrements_the_current_position() {
        let (mut player, now) = playing(&[A, B, C], 1);
        let edits = player.remove(0, now);
        assert_eq!(edits.command, None);
        let queue = edits.queue.unwrap();
        assert_eq!(queue.songs, vec![song(B), song(C)]);
        assert_eq!(queue.current, Some(0));
        assert_eq!(player.state().track, Some(song(B)));
    }

    #[test]
    fn queue_review_moves_upcoming_with_history_but_rejects_moves_across_current() {
        const D: &str = "test-vid-0D";
        let (mut player, now) = playing(&[A, B, C, D], 1);
        let edits = player.move_item(3, 2, now);
        assert_eq!(edits.command, None);
        let queue = edits.queue.unwrap();
        assert_eq!(queue.songs, vec![song(A), song(B), song(D), song(C)]);
        assert_eq!(queue.current, Some(1));
        for (from, to) in [(2, 0), (2, 1), (0, 2), (1, 2)] {
            assert_eq!(player.move_item(from, to, now), Outcome::default());
            assert_eq!(player.queue_state(), queue);
        }
    }

    #[test]
    fn queue_review_enqueue_keeps_the_radio_continuation() {
        let (mut player, now) = playing(&[A], 0);
        player.set_radio_continuation(Some("test-continuation".into()));
        let before_edit = player.queue_position().0;
        let radio_generation = player.radio_generation();

        assert!(player.enqueue(vec![song(B)], now).queue.is_some());
        assert_ne!(player.queue_position().0, before_edit);
        assert_eq!(player.radio_generation(), radio_generation);
        assert_eq!(
            player.needs_more(),
            Some(Seed::Continuation("test-continuation".into()))
        );
        player.clear_upcoming(now);
        assert_ne!(player.radio_generation(), radio_generation);
        assert!(player.radio_continuation.is_none());
    }

    #[test]
    fn staged_queue_waits_for_play_and_explicit_radio_ignores_repeat_for_first_fill() {
        let mut player = Player::new(1);
        let now = Instant::now();
        let staged = player.enqueue(vec![song(A), song(A), song("invalid"), song(B)], now);
        assert_eq!(staged.command, None);
        assert_eq!(staged.queue.unwrap().songs, vec![song(A), song(B)]);
        assert_eq!(player.needs_more(), None);
        assert_eq!(player.play(now).command, Some(WebCommand::Load(id(A))));
        player.set_repeat(RepeatMode::One, now);
        assert_eq!(
            player.start_radio(song(C), now).unwrap().command,
            Some(WebCommand::Load(id(C)))
        );
        assert_eq!(player.queue_state().songs, vec![song(C)]);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(C))));
        player.enqueue(vec![song(A), song(B)], now);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(C))));
        player.set_radio_continuation(Some("test-continuation".into()));
        assert_eq!(player.needs_more(), None);
    }

    #[test]
    fn jumping_to_the_first_staged_song_changes_the_autoplay_trigger() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player.enqueue(vec![song(A)], now);
        let staged = player.autoplay_trigger();
        assert_eq!(
            player.jump_to(0, now).command,
            Some(WebCommand::Load(id(A)))
        );
        assert_ne!(player.autoplay_trigger(), staged);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(A))));
    }

    #[test]
    fn repeat_off_shuffle_and_ended_controls_trigger_new_autoplay_checks() {
        let (mut player, now) = playing(&[A], 0);
        player.set_repeat(RepeatMode::One, now);
        let prior = player.autoplay_trigger();
        player.set_repeat(RepeatMode::Off, now);
        assert_ne!(player.autoplay_trigger(), prior);
        let prior = player.autoplay_trigger();
        player.set_shuffle(true, now);
        assert_ne!(player.autoplay_trigger(), prior);
        player.clear_upcoming(now);
        assert_eq!(player.needs_more(), None);
        player.handle_event(ended(A), now);
        let prior = player.autoplay_trigger();
        assert_eq!(player.next(now), Outcome::default());
        assert_ne!(player.autoplay_trigger(), prior);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(A))));
        let prior = player.autoplay_trigger();
        assert_eq!(player.play(now).command, Some(WebCommand::Load(id(A))));
        assert_ne!(player.autoplay_trigger(), prior);
    }

    #[test]
    fn autoplay_needs_fewer_than_three_upcoming_songs_and_repeat_off() {
        let d = "test-vid-0D";
        let (mut player, now) = playing(&[A, B, C, d], 0);
        assert_eq!(player.needs_more(), None);
        player.next(now);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(d))));
        for repeat in [RepeatMode::One, RepeatMode::All] {
            player.set_repeat(repeat, now);
            assert_eq!(player.needs_more(), None);
        }
        player.stop(now);
        assert_eq!(player.needs_more(), None);
    }

    #[test]
    fn autoplay_uses_the_play_order_tail_and_resets_continuation_on_replace() {
        let (mut player, now) = playing(&[A, B, C], 2);
        player.set_shuffle(true, now);
        let tail = player.queue_state().songs.pop().unwrap();
        assert_ne!(tail.video_id, C);
        assert_eq!(player.needs_more(), Some(Seed::Video(id(&tail.video_id))));
        player.set_radio_continuation(Some("mock-continuation".into()));
        assert_eq!(
            player.needs_more(),
            Some(Seed::Continuation("mock-continuation".into()))
        );
        player.play_songs(vec![song(B)], 0, now).unwrap();
        assert_eq!(player.needs_more(), Some(Seed::Video(id(B))));
    }

    #[test]
    fn autoplay_append_publishes_queue_and_state_without_a_web_command() {
        let (mut player, now) = playing(&[A], 0);
        assert!(!player.state().has_next);
        let outcome = player.append(vec![song(A), song(B)], now);
        assert_eq!(outcome.command, None);
        assert_eq!(outcome.state.unwrap(), player.state());
        assert!(player.state().has_next);
        assert_eq!(outcome.queue.unwrap().songs, vec![song(A), song(B)]);
        assert_eq!(player.append(vec![song(B)], now), Outcome::default());
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
    fn ad_flips_without_a_status_change_are_throttled() {
        let (mut player, now) = playing(&[A], 0);
        let emits = [true, false, true, false]
            .into_iter()
            .enumerate()
            .filter(|&(i, ad)| {
                player
                    .handle_event(
                        PlaybackEvent::Progress {
                            video_id: id(A),
                            playing: true,
                            position: 1.0,
                            duration: 15.0,
                            ad,
                        },
                        now + Duration::from_millis(i as u64 * 50),
                    )
                    .state
                    .is_some()
            })
            .count();
        assert_eq!(emits, 1);
        assert!(!player.state().is_ad, "throttling still updates the state");
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
    fn progress_clamps_position_to_the_bounded_duration() {
        let (mut player, now) = playing(&[A], 0);
        for (duration, expected) in [(1e30, MAX_MEDIA_SECONDS), (200.0, 200.0)] {
            player.handle_event(
                PlaybackEvent::Progress {
                    video_id: id(A),
                    playing: true,
                    position: 1e30,
                    duration,
                    ad: false,
                },
                now,
            );
            let state = player.state();
            assert_eq!((state.position, state.duration), (expected, expected));
        }
    }

    #[test]
    fn seek_clamps_to_the_limit_when_duration_is_unknown() {
        let mut player = Player::new(1);
        let now = Instant::now();
        player.play_songs(vec![song(A)], 0, now).unwrap();
        let outcome = player.seek(1e30, now);
        assert_eq!(outcome.command, Some(WebCommand::Seek(MAX_MEDIA_SECONDS)));
        assert_eq!(player.state().position, MAX_MEDIA_SECONDS);
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
        assert!(
            player
                .handle_event(PlaybackEvent::Failed { video_id: id(A) }, now)
                .state
                .is_none()
        );
    }

    #[test]
    fn stop_empties_the_queue_and_closes_the_page() {
        let (mut player, now) = playing(&[A, B], 0);
        let outcome = player.stop(now);
        assert_eq!(outcome.command, Some(WebCommand::Stop));
        assert_eq!(outcome.state.unwrap().status, PlaybackStatus::Idle);
        assert!(outcome.queue.unwrap().songs.is_empty());
        assert_eq!(player.handle_event(ended(A), now), Outcome::default());
    }

    #[test]
    fn stop_resets_the_player_and_publishes_stopped_inside_the_throttle_window() {
        use crate::player::media::{MediaPublisher, Playback};

        let (mut player, now) = playing(&[A, B], 0);
        player.handle_event(progress(A, true, 7.0), now);
        let mut media = MediaPublisher::default();
        media.update(&player.state(), now);

        let outcome = player.stop(now);
        let state = outcome.state.unwrap();
        assert_eq!(state.status, PlaybackStatus::Idle);
        assert_eq!(state.track, None);
        assert_eq!((state.position, state.duration), (0.0, 0.0));
        assert_eq!(state, player.state());
        let update = media.update(&state, now);
        assert_eq!(update.playback, Some(Playback::Stopped));
        assert_eq!(update.metadata, Some(None));
        let queue = QueueState {
            songs: Vec::new(),
            current: None,
            browse_track: None,
        };
        assert_eq!(outcome.queue, Some(queue.clone()));
        assert_eq!(player.queue_state(), queue);
    }

    #[test]
    fn browse_queue_reports_duplicate_track_row_across_shuffle_and_invalid_rows() {
        let (mut player, now) = playing(&[A], 0);
        let tracks = vec![song(A), song("invalid"), song(A), song(B)];
        let origin = BrowsePlayback {
            browse_id: "test-playlist".into(),
            offset: 500,
            shuffle: true,
        };
        let outcome = player.play_browse_songs(tracks, 2, origin, now).unwrap();
        let queue = outcome.queue.unwrap();
        assert_eq!(queue.current, Some(0));
        assert_eq!(queue.browse_track.unwrap().index, 502);
        player.set_shuffle(false, now);
        let queue = player.queue_state();
        assert_eq!(queue.current, Some(1));
        assert_eq!(queue.browse_track.unwrap().index, 502);
        player.play_songs(vec![song(A)], 0, now).unwrap();
        assert!(player.queue_state().browse_track.is_none());
    }

    #[test]
    fn queue_edits_drop_the_browse_row_mapping() {
        let origin = || BrowsePlayback {
            browse_id: "test-playlist".into(),
            offset: 0,
            shuffle: false,
        };
        let start = || {
            let (mut player, now) = playing(&[A], 0);
            let tracks = vec![song(A), song(B)];
            player.play_browse_songs(tracks, 0, origin(), now).unwrap();
            assert!(player.queue_state().browse_track.is_some());
            (player, now)
        };
        let (mut player, now) = start();
        player.insert_next(vec![song("test-vid-0C")], now);
        assert!(player.queue_state().browse_track.is_none());
        let (mut player, now) = start();
        player.move_item(1, 0, now);
        player.remove(1, now);
        assert!(player.queue_state().browse_track.is_none());
        let (mut player, now) = start();
        player.clear_upcoming(now);
        assert!(player.queue_state().browse_track.is_none());
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
