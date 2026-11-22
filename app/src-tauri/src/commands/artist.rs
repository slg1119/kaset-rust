use tauri::{AppHandle, State};
use ytm_core::models::{ArtistPage, ShelfItem};
use ytm_core::parsers::artist::{parse_artist, parse_artist_items};
use ytm_core::parsers::radio::parse_radio;

use super::{checked_browse_value, signed_in_request};
use crate::error::{AppError, ErrorKind};
use crate::player::commands::validate_queue;
use crate::player::queue::MAX_QUEUE_LEN;
use crate::player::video_id::VideoId;
use crate::session::AuthStatus;
use crate::state::AppState;

/// Fetches the artist header and shelves.
#[tauri::command]
#[specta::specta]
pub async fn get_artist(
    app: AppHandle,
    state: State<'_, AppState>,
    browse_id: String,
) -> Result<ArtistPage, AppError> {
    checked_artist_browse(&browse_id, None)?;
    signed_in_request(&app, &state, |client| async move {
        let mut page = parse_artist(&client.browse(&browse_id).await?)?;
        if page.channel_id.is_none() && browse_id.starts_with("UC") {
            page.channel_id = Some(browse_id);
        }
        Ok(page)
    })
    .await
}

/// Fetches the grid or track list behind an artist shelf's See all button.
#[tauri::command]
#[specta::specta]
pub async fn get_artist_more(
    app: AppHandle,
    state: State<'_, AppState>,
    browse_id: String,
    params: Option<String>,
) -> Result<Vec<ShelfItem>, AppError> {
    checked_artist_browse(&browse_id, params.as_deref())?;
    signed_in_request(&app, &state, |client| async move {
        let response = client
            .browse_with_params(&browse_id, params.as_deref())
            .await?;
        Ok(parse_artist_items(&response)?)
    })
    .await
}

/// Resolves an artist radio or shuffle endpoint and starts its queue.
#[tauri::command]
#[specta::specta]
pub async fn play_artist_mix(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: Option<String>,
    video_id: Option<String>,
    params: Option<String>,
) -> Result<(), AppError> {
    checked_artist_mix(
        playlist_id.as_deref(),
        video_id.as_deref(),
        params.as_deref(),
    )?;
    let epoch = state.session.epoch();
    let generation = state.player.queue_generation().await;
    let mut queue = signed_in_request(&app, &state, |client| async move {
        let response = client
            .playback_queue(
                playlist_id.as_deref(),
                video_id.as_deref(),
                params.as_deref(),
            )
            .await?;
        Ok(parse_radio(&response)?)
    })
    .await?;
    queue
        .songs
        .retain(|song| VideoId::parse(&song.video_id).is_some());
    queue.songs.truncate(MAX_QUEUE_LEN);
    validate_queue(&queue.songs)?;

    state
        .player
        .update(&app, |player, now| {
            if state.session.epoch() != epoch || state.session.status() != AuthStatus::SignedIn {
                return Err(AppError::not_signed_in());
            }
            // A newer queue may have been started while the mix was loading.
            if player.queue_position().0 != generation {
                return Ok(Default::default());
            }
            let outcome = player
                .play_songs(queue.songs, 0, now)
                .map_err(|error| AppError::new(ErrorKind::InvalidArgument, error.to_string()))?;
            player.set_radio_continuation(queue.continuation);
            Ok(outcome)
        })
        .await
}

fn checked_artist_browse(browse_id: &str, params: Option<&str>) -> Result<(), AppError> {
    checked_browse_value(browse_id, 1..=256, b"", "Invalid browse ID.")?;
    checked_artist_params(params)
}

fn checked_artist_params(params: Option<&str>) -> Result<(), AppError> {
    if let Some(params) = params {
        checked_browse_value(params, 1..=4096, b"%=", "Invalid artist parameter.")?;
    }
    Ok(())
}

fn checked_artist_mix(
    playlist_id: Option<&str>,
    video_id: Option<&str>,
    params: Option<&str>,
) -> Result<(), AppError> {
    if playlist_id.is_none() && video_id.is_none() {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            "An artist mix needs a playlist or video ID.",
        ));
    }
    if let Some(playlist_id) = playlist_id {
        checked_browse_value(playlist_id, 1..=256, b"", "Invalid playlist ID.")?;
    }
    if video_id.is_some_and(|id| VideoId::parse(id).is_none()) {
        return Err(AppError::new(
            ErrorKind::InvalidArgument,
            "Invalid video ID.",
        ));
    }
    checked_artist_params(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_artist_browse_arguments() {
        assert!(checked_artist_browse("a", None).is_ok());
        assert!(checked_artist_browse(&"A".repeat(256), Some(&"a".repeat(4096))).is_ok());
        assert!(checked_artist_browse("UC_test-artist", Some("a%3D_-=")).is_ok());
        for id in ["", "UC/invalid", "é", "test%3D", &"A".repeat(257)] {
            assert_eq!(
                checked_artist_browse(id, None).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
        }
        for params in ["", "a+b", "a/b", "a b", "a\n", "é", &"a".repeat(4097)] {
            assert_eq!(
                checked_artist_browse("UC-test", Some(params))
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidArgument
            );
        }
    }

    #[test]
    fn validates_artist_mix_arguments() {
        assert!(checked_artist_mix(Some("RD-test"), None, Some("test%3D")).is_ok());
        assert!(checked_artist_mix(None, Some("test-vid-01"), None).is_ok());
        for arguments in [
            (None, None, None),
            (Some("RD/test"), None, None),
            (None, Some("a"), None),
            (Some("RD-test"), None, Some("a+b")),
        ] {
            assert_eq!(
                checked_artist_mix(arguments.0, arguments.1, arguments.2)
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidArgument
            );
        }
    }
}
