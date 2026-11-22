//! The play queue. Pure Rust: no Tauri types, deterministic for a given seed.

use serde::{Deserialize, Serialize};
use ytm_core::models::Song;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    /// Wrap from the last song to the first.
    All,
    /// Replay the current song when it ends. Next and previous still move.
    One,
}

/// Why the queue moves forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advance {
    /// Next button, media key.
    User,
    /// The current song finished playing.
    Ended,
}

/// xorshift64: enough to shuffle a queue, no dependency.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn below(&mut self, bound: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % bound as u64) as usize
    }
}

#[derive(Debug, Clone)]
pub struct Queue {
    /// Songs in the order they were given.
    songs: Vec<Song>,
    /// Play order: indices into `songs`. The identity unless shuffled.
    order: Vec<usize>,
    /// Index into `order` of the current song.
    position: Option<usize>,
    repeat: RepeatMode,
    shuffle: bool,
    rng: Rng,
}

impl Queue {
    pub fn new(seed: u64) -> Self {
        Self {
            songs: Vec::new(),
            order: Vec::new(),
            position: None,
            repeat: RepeatMode::Off,
            shuffle: false,
            rng: Rng::new(seed),
        }
    }

    /// Replaces the queue and makes `songs[start]` current. With shuffle on,
    /// the start song plays first and the rest are shuffled. An out-of-range
    /// `start` empties the queue.
    pub fn replace(&mut self, songs: Vec<Song>, start: usize) {
        if start >= songs.len() {
            self.clear();
            return;
        }
        self.songs = songs;
        self.order = (0..self.songs.len()).collect();
        self.position = Some(start);
        if self.shuffle {
            self.shuffle_around_current();
        }
    }

    pub fn clear(&mut self) {
        self.songs.clear();
        self.order.clear();
        self.position = None;
    }

    pub fn current(&self) -> Option<&Song> {
        self.position
            .map(|position| &self.songs[self.order[position]])
    }

    /// Index of the current song in play order.
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// Songs in play order.
    pub fn ordered(&self) -> Vec<Song> {
        self.order
            .iter()
            .map(|&index| self.songs[index].clone())
            .collect()
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Turning shuffle on keeps the current song and shuffles the others
    /// after it; turning it off restores the original order.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        if shuffle == self.shuffle {
            return;
        }
        self.shuffle = shuffle;
        if shuffle {
            self.shuffle_around_current();
        } else {
            let current = self.position.map(|position| self.order[position]);
            self.order = (0..self.songs.len()).collect();
            self.position = current;
        }
    }

    /// Moves `order` so the current song comes first and shuffles the rest.
    fn shuffle_around_current(&mut self) {
        let Some(position) = self.position else {
            return;
        };
        let current = self.order.remove(position);
        let mut rest = std::mem::take(&mut self.order);
        // Fisher–Yates.
        for index in (1..rest.len()).rev() {
            let other = self.rng.below(index + 1);
            rest.swap(index, other);
        }
        self.order = std::iter::once(current).chain(rest).collect();
        self.position = Some(0);
    }

    pub fn has_next(&self) -> bool {
        self.position.is_some_and(|position| {
            position + 1 < self.order.len() || self.repeat == RepeatMode::All
        })
    }

    pub fn has_previous(&self) -> bool {
        self.position
            .is_some_and(|position| position > 0 || self.repeat == RepeatMode::All)
    }

    /// Moves to the next song and returns it. `Ended` with `RepeatMode::One`
    /// stays on the current song. At the end of the queue this wraps with
    /// `RepeatMode::All` and otherwise returns `None` without moving.
    pub fn advance(&mut self, reason: Advance) -> Option<&Song> {
        let position = self.position?;
        if reason == Advance::Ended && self.repeat == RepeatMode::One {
            return self.current();
        }
        let next = if position + 1 < self.order.len() {
            position + 1
        } else if self.repeat == RepeatMode::All {
            0
        } else {
            return None;
        };
        self.position = Some(next);
        self.current()
    }

    /// Moves to the previous song and returns it. At the start this wraps with
    /// `RepeatMode::All` and otherwise returns `None` without moving.
    pub fn retreat(&mut self) -> Option<&Song> {
        let position = self.position?;
        let previous = if position > 0 {
            position - 1
        } else if self.repeat == RepeatMode::All {
            self.order.len() - 1
        } else {
            return None;
        };
        self.position = Some(previous);
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song {
            video_id: id.into(),
            title: format!("Song {id}"),
            artists: Vec::new(),
            thumbnail_url: None,
        }
    }

    fn songs(count: usize) -> Vec<Song> {
        (0..count)
            .map(|index| song(&format!("test-vid-{index:02}")))
            .collect()
    }

    fn current_id(queue: &Queue) -> Option<&str> {
        queue.current().map(|song| song.video_id.as_str())
    }

    #[test]
    fn replace_starts_at_the_given_song() {
        let mut queue = Queue::new(1);
        queue.replace(songs(3), 1);
        assert_eq!(current_id(&queue), Some("test-vid-01"));
        assert_eq!(queue.position(), Some(1));
        assert_eq!(queue.ordered(), songs(3));
    }

    #[test]
    fn replace_with_an_invalid_start_empties_the_queue() {
        let mut queue = Queue::new(1);
        queue.replace(songs(3), 0);
        queue.replace(songs(3), 3);
        assert!(queue.current().is_none());
        assert!(queue.ordered().is_empty());
        queue.replace(Vec::new(), 0);
        assert!(queue.current().is_none());
    }

    #[test]
    fn advance_and_retreat_stop_at_the_ends_without_repeat() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 0);
        assert!(!queue.has_previous());
        assert!(queue.retreat().is_none());
        assert_eq!(current_id(&queue), Some("test-vid-00"));

        assert_eq!(
            queue
                .advance(Advance::User)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
        assert!(!queue.has_next());
        assert!(queue.advance(Advance::User).is_none());
        assert!(queue.advance(Advance::Ended).is_none());
        assert_eq!(current_id(&queue), Some("test-vid-01"));
        assert_eq!(
            queue.retreat().map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 1);
        queue.set_repeat(RepeatMode::All);
        assert!(queue.has_next());
        assert_eq!(
            queue
                .advance(Advance::Ended)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
        assert!(queue.has_previous());
        assert_eq!(
            queue.retreat().map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
    }

    #[test]
    fn repeat_one_replays_on_end_but_next_still_moves() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 0);
        queue.set_repeat(RepeatMode::One);
        assert_eq!(
            queue
                .advance(Advance::Ended)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-00")
        );
        assert_eq!(
            queue
                .advance(Advance::User)
                .map(|song| song.video_id.as_str()),
            Some("test-vid-01")
        );
        assert!(queue.advance(Advance::User).is_none());
    }

    #[test]
    fn shuffle_keeps_the_current_song_first_and_permutes_the_rest() {
        let mut queue = Queue::new(42);
        queue.replace(songs(8), 3);
        queue.set_shuffle(true);
        assert_eq!(queue.position(), Some(0));
        assert_eq!(current_id(&queue), Some("test-vid-03"));
        let mut ids: Vec<String> = queue
            .ordered()
            .into_iter()
            .map(|song| song.video_id)
            .collect();
        assert_eq!(ids[0], "test-vid-03");
        ids.sort();
        let mut expected: Vec<String> = songs(8).into_iter().map(|song| song.video_id).collect();
        expected.sort();
        assert_eq!(ids, expected);
        assert_ne!(queue.ordered(), songs(8), "seed 42 changes the order");
    }

    #[test]
    fn shuffle_off_restores_order_and_keeps_the_current_song() {
        let mut queue = Queue::new(42);
        queue.replace(songs(8), 3);
        queue.set_shuffle(true);
        queue.advance(Advance::User);
        let playing = current_id(&queue).unwrap().to_owned();
        queue.set_shuffle(false);
        assert_eq!(queue.ordered(), songs(8));
        assert_eq!(current_id(&queue), Some(playing.as_str()));
    }

    #[test]
    fn replace_while_shuffled_plays_the_start_song_first() {
        let mut queue = Queue::new(7);
        queue.set_shuffle(true);
        queue.replace(songs(5), 2);
        assert_eq!(queue.position(), Some(0));
        assert_eq!(current_id(&queue), Some("test-vid-02"));
    }

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let order = |seed| {
            let mut queue = Queue::new(seed);
            queue.replace(songs(10), 0);
            queue.set_shuffle(true);
            queue.ordered()
        };
        assert_eq!(order(5), order(5));
    }
}
