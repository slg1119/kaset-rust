use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ytm_core::models::FeedPage;
use ytm_core::parsers::home::{parse_feed, parse_feed_continuation};

use super::{checked_browse_id, checked_token, signed_in_request};
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Home,
    Explore,
    Charts,
    MoodsAndGenres,
    NewReleases,
    History,
}

impl FeedKind {
    fn browse_id(self) -> &'static str {
        match self {
            Self::Home => "FEmusic_home",
            Self::Explore => "FEmusic_explore",
            Self::Charts => "FEmusic_charts",
            Self::MoodsAndGenres => "FEmusic_moods_and_genres",
            Self::NewReleases => "FEmusic_new_releases",
            Self::History => "FEmusic_history",
        }
    }
}

/// Fetches the first page of a signed-in music feed.
#[tauri::command]
#[specta::specta]
pub async fn get_feed(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: FeedKind,
) -> Result<FeedPage, AppError> {
    fetch_feed(&app, &state, kind.browse_id(), None).await
}

/// Fetches the first page of a mood or genre category.
#[tauri::command]
#[specta::specta]
pub async fn get_feed_category(
    app: AppHandle,
    state: State<'_, AppState>,
    browse_id: String,
    params: Option<String>,
) -> Result<FeedPage, AppError> {
    checked_feed_category(&browse_id, params.as_deref())?;
    fetch_feed(&app, &state, &browse_id, params.as_deref()).await
}

/// Fetches Home filtered by a Music chip.
#[tauri::command]
#[specta::specta]
pub async fn get_feed_chip(
    app: AppHandle,
    state: State<'_, AppState>,
    params: String,
) -> Result<FeedPage, AppError> {
    checked_feed_value(&params)?;
    fetch_feed(&app, &state, FeedKind::Home.browse_id(), Some(&params)).await
}

/// Fetches the next page of any music feed.
#[tauri::command]
#[specta::specta]
pub async fn get_feed_continuation(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<FeedPage, AppError> {
    checked_feed_value(&token)?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_feed_continuation(
            &client.browse_continuation(&token).await?,
        )?)
    })
    .await
}

async fn fetch_feed(
    app: &AppHandle,
    state: &AppState,
    browse_id: &str,
    params: Option<&str>,
) -> Result<FeedPage, AppError> {
    signed_in_request(app, state, |client| async move {
        Ok(parse_feed(
            &client.browse_with_params(browse_id, params).await?,
        )?)
    })
    .await
}

fn checked_feed_category(browse_id: &str, params: Option<&str>) -> Result<(), AppError> {
    checked_browse_id(browse_id)?;
    if let Some(params) = params {
        checked_feed_value(params)?;
    }
    Ok(())
}

pub(super) fn checked_feed_value(value: &str) -> Result<(), AppError> {
    checked_token(value, "Invalid feed parameter or continuation.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn feed_kind_maps_to_browse_ids() {
        for (kind, id) in [
            (FeedKind::Home, "FEmusic_home"),
            (FeedKind::Explore, "FEmusic_explore"),
            (FeedKind::Charts, "FEmusic_charts"),
            (FeedKind::MoodsAndGenres, "FEmusic_moods_and_genres"),
            (FeedKind::NewReleases, "FEmusic_new_releases"),
            (FeedKind::History, "FEmusic_history"),
        ] {
            assert_eq!(kind.browse_id(), id);
        }
    }

    #[test]
    fn feed_arguments_enforce_the_ipc_contract() {
        assert!(checked_feed_category("A", None).is_ok());
        assert!(checked_feed_category(&"A".repeat(256), Some("a_-%3D=+/.-")).is_ok());
        assert!(checked_feed_value(&"a".repeat(4096)).is_ok());
        assert!(checked_feed_value("4qmFsgK+/a.b%3D=").is_ok());
        for id in ["", "bad/id", "id%20", "테스트", &"A".repeat(257)] {
            assert_eq!(
                checked_feed_category(id, None).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
        }
        for value in ["", "a?b", "a:b", "a b", " a", "a\n", "é", &"a".repeat(4097)] {
            assert_eq!(
                checked_feed_value(value).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
            assert_eq!(
                checked_feed_category("test-id", Some(value))
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidArgument
            );
        }
    }
}
