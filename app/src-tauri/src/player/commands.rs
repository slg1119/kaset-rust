//! Player commands. The UI calls all but `playback_event`, which only the
//! player window's music.youtube.com pages may call (capability `player-remote`).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, Webview};
use ytm_core::models::Song;

use super::engine::{PlaybackEvent, PlayerState, QueueState};
use super::queue::RepeatMode;
use super::window::{PLAYER_LABEL, is_music_page};
use crate::error::{AppError, ErrorKind};
use crate::session::AuthStatus;
use crate::state::AppState;

/// Longest queue `play_queue` accepts.
pub const MAX_QUEUE_LEN: usize = 500;
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
    let start = usize::try_from(start_index).unwrap_or(usize::MAX);
    state
        .player
        .update(&app, |player, now| {
            if state.session.status() != AuthStatus::SignedIn {
                return Err(AppError::not_signed_in());
            }
            validate_queue(&songs)?;
            player
                .play_songs(songs, start, now)
                .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))
        })
        .await
}

fn validate_queue(songs: &[Song]) -> Result<(), AppError> {
    if songs.len() > MAX_QUEUE_LEN {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            format!("A queue holds at most {MAX_QUEUE_LEN} songs."),
        ));
    }
    let too_long = |value: &str| value.len() > MAX_SONG_STRING_BYTES;
    if songs.iter().any(|song| {
        too_long(&song.video_id)
            || too_long(&song.title)
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
