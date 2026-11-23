use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use ytm_core::client::InnerTubeClient;
use ytm_core::models::{AddToPlaylistMenu, LikeStatus, PlaylistPrivacy, SongMetadata};
use ytm_core::secret::Secret;

use super::{checked_browse_value, checked_token, signed_in_request_with_epoch};
use crate::error::{AppError, ErrorKind};
use crate::session::AuthStatus;
use crate::state::AppState;
use crate::webview_session::blocking;

const MAX_CACHED_SONGS: usize = 128;

#[derive(Clone, Copy, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAction {
    Add,
    Remove,
}

#[derive(Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct LibraryEdit {
    video_id: String,
    action: LibraryAction,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct SongActionStatus {
    like_status: Option<LikeStatus>,
    in_library: bool,
    can_add: bool,
    can_remove: bool,
}

/// Accessed under one async lock by metadata reads and feedback writes. This
/// prevents an older metadata response from reintroducing consumed tokens.
#[derive(Default)]
pub(crate) struct FeedbackCache {
    epoch: u64,
    entries: VecDeque<(String, SongMetadata)>,
}

impl FeedbackCache {
    fn select_epoch(&mut self, epoch: u64) -> bool {
        if epoch < self.epoch {
            return false;
        }
        if epoch != self.epoch {
            self.entries.clear();
            self.epoch = epoch;
        }
        true
    }

    fn insert(
        &mut self,
        epoch: u64,
        video_id: &str,
        mut metadata: SongMetadata,
    ) -> SongActionStatus {
        let valid =
            |token: &Secret| checked_token(token.expose(), "Invalid feedback token.").is_ok();
        metadata.add_token = metadata.add_token.filter(valid);
        metadata.remove_token = metadata.remove_token.filter(valid);
        let visible = SongActionStatus {
            like_status: metadata.like_status,
            in_library: metadata.in_library,
            can_add: metadata.add_token.is_some(),
            can_remove: metadata.remove_token.is_some(),
        };
        if self.select_epoch(epoch) {
            self.remove(video_id);
            if self.entries.len() == MAX_CACHED_SONGS {
                self.entries.pop_front();
            }
            self.entries.push_back((video_id.to_owned(), metadata));
        }
        visible
    }

    fn token(
        &mut self,
        epoch: u64,
        video_id: &str,
        action: LibraryAction,
    ) -> Result<&Secret, AppError> {
        if !self.select_epoch(epoch) {
            return Err(AppError::not_signed_in());
        }
        self.entries
            .iter()
            .find(|(id, _)| id == video_id)
            .and_then(|(_, metadata)| match action {
                LibraryAction::Add if !metadata.in_library => metadata.add_token.as_ref(),
                LibraryAction::Remove if metadata.in_library => metadata.remove_token.as_ref(),
                _ => None,
            })
            .ok_or_else(|| invalid("Reload the song’s library status and try again."))
    }

    fn remove(&mut self, video_id: &str) {
        self.entries.retain(|(id, _)| id != video_id);
    }
}

async fn account_request<T>(
    app: &AppHandle,
    state: &AppState,
    run: impl AsyncFnOnce(InnerTubeClient, u64) -> Result<T, AppError>,
) -> Result<T, AppError> {
    signed_in_request_with_epoch(app, state, |client, epoch| async move {
        let handle = app.clone();
        let cookies = blocking(move || handle.state::<AppState>().session.cookies_for_epoch(epoch))
            .await?
            .ok_or_else(AppError::not_signed_in)?;
        run(client.with_cookie_snapshot(cookies), epoch).await
    })
    .await
}

fn invalid(message: &'static str) -> AppError {
    AppError::new(ErrorKind::InvalidArgument, message)
}

fn checked_id(value: &str, message: &'static str) -> Result<(), AppError> {
    checked_browse_value(value, 1..=256, b"", message)
}

fn checked_playlist_id(id: &str) -> Result<(), AppError> {
    checked_id(id, "Invalid playlist ID.")?;
    checked_id(id.strip_prefix("VL").unwrap_or(id), "Invalid playlist ID.")
}

fn validate_playlist_input(
    title: &str,
    description: &str,
    video_ids: &[String],
) -> Result<(), AppError> {
    if title.trim().is_empty() || title.chars().count() > 150 || title.chars().any(char::is_control)
    {
        return Err(invalid("Enter a playlist title of 1–150 characters."));
    }
    if description.chars().count() > 5_000
        || description
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\r' | '\n' | '\t'))
    {
        return Err(invalid(
            "Enter a playlist description of up to 5,000 characters.",
        ));
    }
    for id in video_ids {
        checked_id(id, "Invalid video ID.")?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn song_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
) -> Result<SongActionStatus, AppError> {
    checked_id(&video_id, "Invalid video ID.")?;
    account_request(&app, &state, async |client, epoch| {
        let mut cache = state.feedback.lock().await;
        if state.session.epoch() != epoch || state.session.status() != AuthStatus::SignedIn {
            return Err(AppError::not_signed_in());
        }
        let metadata = client.song_metadata(&video_id).await?;
        if state.session.epoch() != epoch || state.session.status() != AuthStatus::SignedIn {
            return Err(AppError::not_signed_in());
        }
        Ok(cache.insert(epoch, &video_id, metadata))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn edit_library(
    app: AppHandle,
    state: State<'_, AppState>,
    edit: LibraryEdit,
) -> Result<(), AppError> {
    checked_id(&edit.video_id, "Invalid video ID.")?;
    account_request(&app, &state, async |client, epoch| {
        let mut cache = state.feedback.lock().await;
        if state.session.epoch() != epoch || state.session.status() != AuthStatus::SignedIn {
            return Err(AppError::not_signed_in());
        }
        let token = cache.token(epoch, &edit.video_id, edit.action)?;
        client.edit_library(token).await?;
        cache.remove(&edit.video_id);
        Ok(())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn rate_song(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
    rating: LikeStatus,
) -> Result<(), AppError> {
    checked_id(&video_id, "Invalid video ID.")?;
    account_request(&app, &state, async move |client, _| {
        Ok(client.rate_song(&video_id, rating).await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn add_to_playlist_options(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
) -> Result<AddToPlaylistMenu, AppError> {
    checked_id(&video_id, "Invalid video ID.")?;
    account_request(&app, &state, async move |client, _| {
        Ok(client.add_to_playlist_options(&video_id).await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn add_to_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: String,
    playlist_id: String,
) -> Result<(), AppError> {
    checked_id(&video_id, "Invalid video ID.")?;
    checked_playlist_id(&playlist_id)?;
    account_request(&app, &state, async move |client, _| {
        Ok(client.add_to_playlist(&video_id, &playlist_id).await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_from_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    video_id: String,
    set_video_id: String,
) -> Result<(), AppError> {
    checked_playlist_id(&playlist_id)?;
    checked_id(&video_id, "Invalid video ID.")?;
    checked_token(&set_video_id, "Invalid playlist item ID.")?;
    account_request(&app, &state, async move |client, _| {
        Ok(client
            .remove_from_playlist(&playlist_id, &video_id, &set_video_id)
            .await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn create_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    description: String,
    privacy: PlaylistPrivacy,
    video_ids: Vec<String>,
) -> Result<String, AppError> {
    validate_playlist_input(&title, &description, &video_ids)?;
    account_request(&app, &state, async move |client, _| {
        Ok(client
            .create_playlist(&title, &description, privacy, &video_ids)
            .await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_playlist(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
) -> Result<(), AppError> {
    checked_playlist_id(&playlist_id)?;
    account_request(&app, &state, async move |client, _| {
        Ok(client.delete_playlist(&playlist_id).await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_artist_subscribed(
    app: AppHandle,
    state: State<'_, AppState>,
    channel_id: String,
    subscribed: bool,
) -> Result<(), AppError> {
    checked_id(&channel_id, "Invalid channel ID.")?;
    account_request(&app, &state, async move |client, _| {
        Ok(client
            .set_artist_subscribed(&channel_id, subscribed)
            .await?)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_playlist_saved(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_id: String,
    saved: bool,
) -> Result<(), AppError> {
    checked_playlist_id(&playlist_id)?;
    account_request(&app, &state, async move |client, _| {
        Ok(client.set_playlist_saved(&playlist_id, saved).await?)
    })
    .await
}

#[cfg(test)]
#[path = "account_actions_tests.rs"]
mod tests;
