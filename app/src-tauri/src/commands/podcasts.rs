use tauri::{AppHandle, State};
use ytm_core::models::{FeedPage, PodcastEpisodesPage, PodcastShow};
use ytm_core::parsers::podcasts::{
    PODCASTS_BROWSE_ID, parse_podcast_episodes_continuation, parse_podcast_show, parse_podcasts,
};

use super::feed::checked_feed_value;
use super::{checked_browse_id, signed_in_request};
use crate::error::AppError;
use crate::state::AppState;

/// Fetches the podcast discovery feed. Further shelves use get_feed_continuation.
#[tauri::command]
#[specta::specta]
pub async fn get_podcasts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FeedPage, AppError> {
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_podcasts(&client.browse(PODCASTS_BROWSE_ID).await?)?)
    })
    .await
}

/// Fetches a podcast show and its first page of episodes.
#[tauri::command]
#[specta::specta]
pub async fn get_podcast_show(
    app: AppHandle,
    state: State<'_, AppState>,
    browse_id: String,
) -> Result<PodcastShow, AppError> {
    checked_browse_id(&browse_id)?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_podcast_show(&client.browse(&browse_id).await?)?)
    })
    .await
}

/// Fetches the next page of episodes for a podcast show.
#[tauri::command]
#[specta::specta]
pub async fn get_podcast_episodes_continuation(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<PodcastEpisodesPage, AppError> {
    checked_feed_value(&token)?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_podcast_episodes_continuation(
            &client.browse_continuation(&token).await?,
        )?)
    })
    .await
}
