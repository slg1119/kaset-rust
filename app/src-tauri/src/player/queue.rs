//! The play queue. Pure Rust: no Tauri types, deterministic for a given seed.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ytm_core::models::Song;

pub const MAX_QUEUE_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub enum QueueAddResult {
    Added,
    AlreadyQueued,
    Full,
}

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

    /// Adds radio tracks that are absent from the entire queue.
    pub fn append(&mut self, songs: Vec<Song>) -> usize {
        self.position
            .and_then(|position| self.append_unique(songs, position, 0))
            .unwrap_or(0)
    }

    /// Current and played tracks can be queued again unless already upcoming.
    pub fn enqueue(&mut self, songs: Vec<Song>) -> QueueAddResult {
        let position = self.position.unwrap_or(self.len());
        let upcoming = self.position.map_or(position, |index| index + 1);
        match self.append_unique(songs, position, upcoming) {
            Some(0) => QueueAddResult::Full,
            Some(count) => {
                if self.position.is_none() {
                    self.position = Some(self.len() - count);
                }
                QueueAddResult::Added
            }
            None => QueueAddResult::AlreadyQueued,
        }
    }

    /// After playback ends, keep the old entries as history and select the
    /// first new occurrence, as when appending after a removed final track.
    pub(super) fn enqueue_after_end(&mut self, songs: Vec<Song>) -> QueueAddResult {
        let previous = self.position.take();
        let result = self.enqueue(songs);
        if result != QueueAddResult::Added {
            self.position = previous;
        }
        result
    }

    /// Only tracks before the current position can be dropped to make room.
    /// Without a current song, every retained track is played history.
    fn append_unique(
        &mut self,
        songs: Vec<Song>,
        position: usize,
        dedupe_from: usize,
    ) -> Option<usize> {
        let capacity = MAX_QUEUE_LEN.saturating_sub(self.len() - position);
        let mut known: HashSet<_> = self.order[dedupe_from..]
            .iter()
            .map(|&index| self.songs[index].video_id.clone())
            .collect();
        let mut unique = songs
            .into_iter()
            .filter(|song| known.insert(song.video_id.clone()))
            .peekable();
        unique.peek()?;
        let added: Vec<_> = unique.take(capacity).collect();
        let count = added.len();
        if count == 0 {
            return Some(0);
        }
        let overflow = (self.songs.len() + count).saturating_sub(MAX_QUEUE_LEN);
        if overflow > 0 {
            self.drop_range(..overflow);
            self.position = self.position.map(|index| index - overflow);
        }
        self.order
            .extend(self.songs.len()..self.songs.len() + count);
        self.songs.extend(added);
        Some(count)
    }

    /// New occurrences share enqueue's deduplication and capacity rules. A song
    /// already upcoming moves into the requested block with its metadata intact.
    pub fn insert_next(&mut self, songs: Vec<Song>) -> QueueAddResult {
        if self.position.is_none() {
            return self.enqueue(songs);
        }
        let mut seen = HashSet::new();
        let songs: Vec<_> = songs
            .into_iter()
            .filter(|song| seen.insert(song.video_id.clone()))
            .collect();
        let ids: Vec<_> = songs.iter().map(|song| song.video_id.clone()).collect();
        let mut result = self.enqueue(songs);
        let Some(position) = self.position else {
            return result;
        };
        let mut destination = position + 1;
        for id in ids {
            if let Some(offset) = self.order[destination..]
                .iter()
                .position(|&index| self.songs[index].video_id == id)
            {
                let entry = self.order.remove(destination + offset);
                self.order.insert(destination, entry);
                destination += 1;
                if offset != 0 {
                    result = QueueAddResult::Added;
                }
            }
        }
        if result == QueueAddResult::Added {
            self.commit_unshuffled_order();
        }
        result
    }

    fn len(&self) -> usize {
        self.order.len()
    }

    pub fn remove(&mut self, index: usize) -> bool {
        let Some(current) = self.position.filter(|_| index < self.len()) else {
            return false;
        };
        self.drop_range(index..=index);
        let next = current - usize::from(index < current);
        self.position = (next < self.len()).then_some(next);
        true
    }

    /// Moves an upcoming song to another upcoming index in play order.
    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        let Some(current) = self.position else {
            return false;
        };
        if from <= current || to <= current || from == to || from >= self.len() || to >= self.len()
        {
            return false;
        }
        let entry = self.order.remove(from);
        self.order.insert(to, entry);
        self.commit_unshuffled_order();
        true
    }

    pub fn jump_to(&mut self, index: usize) -> bool {
        if index >= self.len() {
            return false;
        }
        self.position = Some(index);
        true
    }

    pub fn clear_upcoming(&mut self) -> bool {
        let Some(position) = self.position.filter(|&index| index + 1 < self.len()) else {
            return false;
        };
        self.drop_range(position + 1..);
        true
    }

    fn drop_range(&mut self, range: impl std::ops::RangeBounds<usize>) {
        let dropped: HashSet<_> = self.order.drain(range).collect();
        let mut retained = Vec::with_capacity(self.order.len());
        let mut remap = vec![0; self.songs.len()];
        for (old, song) in self.songs.drain(..).enumerate() {
            if !dropped.contains(&old) {
                remap[old] = retained.len();
                retained.push(song);
            }
        }
        self.songs = retained;
        for index in &mut self.order {
            *index = remap[*index];
        }
    }

    fn commit_unshuffled_order(&mut self) {
        if !self.shuffle {
            self.songs = self.ordered();
            self.order = (0..self.songs.len()).collect();
        }
    }

    pub fn remaining(&self) -> Option<usize> {
        self.position
            .map(|position| self.order.len() - position - 1)
    }

    pub fn last(&self) -> Option<&Song> {
        self.order.last().map(|&index| &self.songs[index])
    }

    pub fn current(&self) -> Option<&Song> {
        self.position
            .map(|position| &self.songs[self.order[position]])
    }

    /// Index of the current song in play order.
    pub fn position(&self) -> Option<usize> {
        self.position
    }

    /// Index of the current song in the original queue order.
    pub fn original_position(&self) -> Option<usize> {
        self.position.map(|position| self.order[position])
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
            let current = self.original_position();
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
            album: None,
            is_podcast_episode: None,
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
    fn edits_use_play_order_and_keep_the_current_song() {
        let mut queue = Queue::new(42);
        queue.replace(songs(6), 2);
        queue.set_shuffle(true);
        let before = queue.ordered();
        assert_eq!(
            queue.insert_next(vec![before[4].clone(), song("new"), song("new")]),
            QueueAddResult::Added
        );
        assert_eq!(
            &queue.ordered()[..3],
            &[before[0].clone(), before[4].clone(), song("new")]
        );
        assert!(queue.move_item(2, 4));
        assert_eq!(queue.ordered()[4], song("new"));
        assert!(queue.remove(4));
        assert!(queue.jump_to(1));
        assert!(queue.clear_upcoming());
        assert_eq!(queue.ordered(), vec![before[0].clone(), before[4].clone()]);
        queue.set_shuffle(false);
        assert_eq!(queue.current(), Some(&before[4]));
        assert_eq!(queue.ordered().len(), 2);
    }

    #[test]
    fn insert_next_at_capacity_reorders_upcoming_and_only_evicts_history() {
        let mut queue = Queue::new(1);
        let original = songs(MAX_QUEUE_LEN);
        queue.replace(original.clone(), 1);
        let later = original[MAX_QUEUE_LEN - 1].clone();
        assert_eq!(
            queue.insert_next(vec![later.clone(), song("new"), song("overflow")]),
            QueueAddResult::Added
        );
        let order = queue.ordered();
        assert_eq!(order.len(), MAX_QUEUE_LEN);
        assert_eq!(queue.position(), Some(0));
        assert_eq!(&order[..3], &[original[1].clone(), later, song("new")]);
        assert!(!order.contains(&song("overflow")));
        assert!(!queue.remove(MAX_QUEUE_LEN));
        assert!(!queue.jump_to(MAX_QUEUE_LEN));
    }

    #[test]
    fn queue_add_reports_duplicates_and_capacity_separately() {
        for add in [Queue::enqueue, Queue::insert_next] {
            let mut queue = Queue::new(1);
            let tracks = songs(MAX_QUEUE_LEN);
            queue.replace(tracks.clone(), 0);
            assert_eq!(
                add(&mut queue, vec![tracks[1].clone()]),
                QueueAddResult::AlreadyQueued
            );
            assert_eq!(add(&mut queue, vec![song("new")]), QueueAddResult::Full);
            assert_eq!(queue.ordered(), tracks);
        }
    }

    #[test]
    fn autoplay_append_deduplicates_in_input_order() {
        let mut queue = Queue::new(1);
        queue.replace(songs(2), 1);
        assert_eq!(
            queue.append(songs(4).into_iter().chain(songs(4)).collect()),
            2
        );
        assert_eq!(queue.ordered(), songs(4));
        assert_eq!(queue.position(), Some(1));
    }

    #[test]
    fn autoplay_append_preserves_the_shuffled_prefix() {
        let mut queue = Queue::new(42);
        queue.replace(songs(6), 2);
        queue.set_shuffle(true);
        queue.advance(Advance::User);
        let current = queue.current().cloned();
        let mut expected = queue.ordered();
        expected.extend(songs(9).into_iter().skip(6));
        assert_eq!(queue.append(songs(9)), 3);
        assert_eq!(queue.ordered(), expected);
        assert_eq!(queue.current().cloned(), current);
        assert_eq!(queue.position(), Some(1));
    }

    #[test]
    fn autoplay_append_caps_at_500_without_dropping_current_or_upcoming() {
        for shuffle in [false, true] {
            let mut queue = Queue::new(42);
            queue.replace(songs(500), 0);
            queue.set_shuffle(shuffle);
            for _ in 0..3 {
                queue.advance(Advance::User);
            }
            let mut expected = queue.ordered().split_off(3);
            expected.extend(songs(503).into_iter().skip(500));
            assert_eq!(queue.append(songs(510)), 3);
            assert_eq!(queue.ordered(), expected);
            assert_eq!(queue.position(), Some(0));
            assert_eq!(queue.current(), expected.first());
            assert_eq!(queue.append(vec![song("extra")]), 0);
            queue.set_shuffle(false);
            assert_eq!(queue.current(), expected.first());
            let mut retained = expected;
            retained.sort_by_key(|song| {
                song.video_id
                    .strip_prefix("test-vid-")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            });
            assert_eq!(queue.ordered(), retained);
        }
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
