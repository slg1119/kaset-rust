//! Player commands. The UI calls all but `playback_event`, which only the
//! player window's music.youtube.com pages may call (capability `player-remote`).

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, Webview};
use ytm_core::models::Song;

use super::engine::{
    BrowsePlayback, Outcome, PlaybackEvent, Player, PlayerError, PlayerState, QueueState,
};
use super::queue::{MAX_QUEUE_LEN, QueueAddResult, RepeatMode};
use super::window::{PLAYER_LABEL, is_music_page};
use crate::commands::checked_browse_id;
use crate::error::{AppError, ErrorKind};
use crate::session::AuthStatus;
use crate::state::AppState;

const MAX_SONG_STRING_BYTES: usize = 2048;

/// Reply to `player_snapshot`: the state the events would have sent so far.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct PlayerSnapshot {
    pub state: PlayerState,
    pub queue: QueueState,
}

/// Replaces the queue with `songs` and plays `songs[start_index]`.
#[tauri::command]
#[specta::specta]
pub async fn play_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    songs: Vec<Song>,
    start_index: u32,
) -> Result<(), AppError> {
    play_queue_inner(&app, &state, songs, start_index, None).await
}

/// Plays tracks from a detail page, retaining the original row through shuffle.
#[tauri::command]
#[specta::specta]
pub async fn play_browse_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    songs: Vec<Song>,
    start_index: u32,
    playback: BrowsePlayback,
) -> Result<(), AppError> {
    checked_browse_id(&playback.browse_id)?;
    if playback
        .offset
        .checked_add(MAX_QUEUE_LEN as u32 - 1)
        .is_none()
    {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            "Invalid track offset.",
        ));
    }
    play_queue_inner(&app, &state, songs, start_index, Some(playback)).await
}

async fn play_queue_inner(
    app: &AppHandle,
    state: &AppState,
    songs: Vec<Song>,
    start_index: u32,
    playback: Option<BrowsePlayback>,
) -> Result<(), AppError> {
    let start = usize::try_from(start_index).unwrap_or(usize::MAX);
    state
        .player
        .update(app, |player, now| {
            if state.session.status() != AuthStatus::SignedIn {
                return Err(AppError::not_signed_in());
            }
            match playback {
                Some(origin) => {
                    validate_queue(&songs)?;
                    player
                        .play_browse_songs(songs, start, origin, now)
                        .map_err(|error| {
                            AppError::new(ErrorKind::InvalidArgument, error.to_string())
                        })
                }
                None => replace_queue(player, songs, start, now),
            }
        })
        .await
}

pub(crate) fn replace_queue(
    player: &mut Player,
    songs: Vec<Song>,
    start: usize,
    now: Instant,
) -> Result<Outcome, AppError> {
    validate_queue(&songs)?;
    player
        .play_songs(songs, start, now)
        .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))
}

pub(crate) fn validate_queue(songs: &[Song]) -> Result<(), AppError> {
    if songs.len() > MAX_QUEUE_LEN {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("A queue holds at most {MAX_QUEUE_LEN} songs."),
        ));
    }
    for album in songs.iter().filter_map(|song| song.album.as_ref()) {
        checked_browse_id(&album.browse_id)?;
        if !album
            .browse_id
            .strip_prefix("MPREb_")
            .is_some_and(|suffix| !suffix.is_empty())
        {
            return Err(AppError::new(
                ErrorKind::InvalidArgument,
                "Invalid album browse ID.",
            ));
        }
    }
    let too_long = |value: &str| value.len() > MAX_SONG_STRING_BYTES;
    if songs.iter().any(|song| {
        too_long(&song.video_id)
            || too_long(&song.title)
            || song
                .album
                .as_ref()
                .is_some_and(|album| too_long(&album.title))
            || song.thumbnail_url.as_deref().is_some_and(too_long)
            || song.artists.iter().any(|artist| {
                too_long(&artist.name) || artist.browse_id.as_deref().is_some_and(too_long)
            })
    }) {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("Song text fields must be at most {MAX_SONG_STRING_BYTES} bytes."),
        ));
    }
    Ok(())
}

/// Moves songs to play next. Reports whether tracks were added, already queued, or blocked by the cap.
#[tauri::command]
#[specta::specta]
pub async fn insert_next(
    app: AppHandle,
    state: State<'_, AppState>,
    songs: Vec<Song>,
) -> Result<QueueAddResult, AppError> {
    validate_queue(&songs)?;
    add_to_queue(&app, &state, |player, now| player.insert_next(songs, now)).await
}

/// Appends songs to the queue. Reports whether tracks were added, already queued, or blocked by the cap.
#[tauri::command]
#[specta::specta]
pub async fn enqueue(
    app: AppHandle,
    state: State<'_, AppState>,
    songs: Vec<Song>,
) -> Result<QueueAddResult, AppError> {
    validate_queue(&songs)?;
    add_to_queue(&app, &state, |player, now| player.enqueue(songs, now)).await
}

async fn add_to_queue(
    app: &AppHandle,
    state: &AppState,
    add: impl FnOnce(&mut Player, Instant) -> Result<(QueueAddResult, Outcome), PlayerError>,
) -> Result<QueueAddResult, AppError> {
    let mut result = QueueAddResult::AlreadyQueued;
    edit_queue(app, state, |player, now| {
        let (added, outcome) = add(player, now).map_err(|_| {
            AppError::new(
                ErrorKind::InvalidArgument,
                "Queue additions require valid video IDs.",
            )
        })?;
        result = added;
        Ok(outcome)
    })
    .await?;
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_queue_item(
    app: AppHandle,
    state: State<'_, AppState>,
    index: u32,
) -> Result<(), AppError> {
    edit_queue(&app, &state, |player, now| {
        Ok(player.remove(index as usize, now))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn move_queue_item(
    app: AppHandle,
    state: State<'_, AppState>,
    from: u32,
    to: u32,
) -> Result<bool, AppError> {
    let mut moved = false;
    edit_queue(&app, &state, |player, now| {
        let outcome = player.move_item(from as usize, to as usize, now);
        moved = outcome.queue.is_some();
        Ok(outcome)
    })
    .await?;
    Ok(moved)
}

#[tauri::command]
#[specta::specta]
pub async fn jump_to(
    app: AppHandle,
    state: State<'_, AppState>,
    index: u32,
) -> Result<(), AppError> {
    edit_queue(&app, &state, |player, now| {
        Ok(player.jump_to(index as usize, now))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn clear_upcoming(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    edit_queue(&app, &state, |player, now| Ok(player.clear_upcoming(now))).await
}

#[tauri::command]
#[specta::specta]
pub async fn start_radio(
    app: AppHandle,
    state: State<'_, AppState>,
    song: Song,
) -> Result<(), AppError> {
    validate_queue(std::slice::from_ref(&song))?;
    edit_queue(&app, &state, |player, now| {
        player
            .start_radio(song, now)
            .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))
    })
    .await
}

async fn edit_queue(
    app: &AppHandle,
    state: &AppState,
    edit: impl FnOnce(&mut Player, Instant) -> Result<Outcome, AppError>,
) -> Result<(), AppError> {
    state
        .player
        .update(app, |player, now| {
            if state.session.status() != AuthStatus::SignedIn {
                return Err(AppError::not_signed_in());
            }
            edit(player, now)
        })
        .await
}

/// Pauses when playing or loading; otherwise plays.
#[tauri::command]
#[specta::specta]
pub async fn toggle_playback(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.toggle(now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn next_track(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.next(now)))
        .await
}

/// Restarts the song after three seconds; otherwise plays the previous song.
#[tauri::command]
#[specta::specta]
pub async fn previous_track(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.previous(now)))
        .await
}

/// Seeks to `position` seconds. Ignored while an ad plays.
#[tauri::command]
#[specta::specta]
pub async fn seek_to(
    app: AppHandle,
    state: State<'_, AppState>,
    position: f64,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.seek(position, now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_repeat(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: RepeatMode,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.set_repeat(mode, now)))
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_shuffle(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), AppError> {
    state
        .player
        .update(&app, |player, now| Ok(player.set_shuffle(enabled, now)))
        .await
}

/// The current player state and queue, for a UI that starts after playback.
#[tauri::command]
#[specta::specta]
pub async fn player_snapshot(state: State<'_, AppState>) -> Result<PlayerSnapshot, AppError> {
    let (state, queue) = state.player.snapshot().await;
    Ok(PlayerSnapshot { state, queue })
}

/// Called by the player window's init script. Rejects calls from any other
/// window or origin, even though the capability already limits callers.
#[tauri::command]
#[specta::specta]
pub async fn playback_event(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AppState>,
    event: PlaybackEvent,
) -> Result<(), AppError> {
    // The top-level URL check is defense in depth. Tauri resolves the ACL
    // against the request Origin and injects IPC into the main frame only.
    let from_player =
        webview.label() == PLAYER_LABEL && webview.url().is_ok_and(|url| is_music_page(&url));
    if !from_player {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            "playback events come only from the player window",
        ));
    }
    state
        .player
        .update(&app, |player, now| Ok(player.handle_event(event, now)))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ytm_core::models::ArtistRef;

    fn song() -> Song {
        Song {
            album: None,
            is_podcast_episode: None,
            video_id: "abc-DEF_123".into(),
            title: "Test song".into(),
            artists: vec![ArtistRef {
                name: "Test artist".into(),
                browse_id: Some("test-artist-id".into()),
            }],
            thumbnail_url: Some("https://example.com/thumbnail.jpg".into()),
        }
    }

    #[test]
    fn rejects_queues_over_500_songs() {
        let mut songs = vec![song(); 500];
        assert!(validate_queue(&songs).is_ok());
        songs.push(song());
        let error = validate_queue(&songs).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidArgument);
    }

    #[test]
    fn validates_optional_album_ids_at_the_queue_boundary() {
        let mut track = song();
        track.album = Some(ytm_core::models::AlbumRef {
            browse_id: "MPREb_test-album".into(),
            title: "Test album".into(),
        });
        assert!(validate_queue(std::slice::from_ref(&track)).is_ok());

        for id in [
            "MPREb_",
            "MPREb_bad/path",
            "UCtest-artist",
            &"MPREb_x".repeat(40),
        ] {
            track.album.as_mut().unwrap().browse_id = id.into();
            assert_eq!(
                validate_queue(std::slice::from_ref(&track))
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidArgument
            );
        }
    }

    #[test]
    fn rejects_song_strings_over_2048_bytes() {
        let fields: [fn(&mut Song) -> &mut String; 5] = [
            |song| &mut song.video_id,
            |song| &mut song.title,
            |song| &mut song.artists[0].name,
            |song| song.artists[0].browse_id.as_mut().unwrap(),
            |song| song.thumbnail_url.as_mut().unwrap(),
        ];
        for field in fields {
            let mut songs = [song(), song()];
            *field(&mut songs[1]) = "é".repeat(1024);
            assert!(validate_queue(&songs).is_ok());
            field(&mut songs[1]).push('a');
            let error = validate_queue(&songs).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidArgument);
        }
    }
}
