use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ytm_core::client::InnerTubeClient;
use ytm_core::models::{BrowsePage, BrowsePageContinuation, HomeSection, ShelfItem};
use ytm_core::parsers::browse_page::{parse_browse_page_continuation, parse_browse_page_for_id};
use ytm_core::parsers::library::{parse_library, parse_library_continuation};

use super::{checked_browse_value, signed_in_request};
use crate::error::AppError;
use crate::state::AppState;

const LIKED_SONGS_ID: &str = "VLLM";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LibraryKind {
    Playlists,
    Albums,
    Artists,
    Songs,
}

impl LibraryKind {
    fn endpoint(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Playlists => ("FEmusic_liked_playlists", None),
            Self::Albums => ("FEmusic_liked_albums", None),
            Self::Artists => ("FEmusic_library_corpus_artists", Some("ggMCCAU=")),
            Self::Songs => (LIKED_SONGS_ID, None),
        }
    }

    fn item_id(self, item: &ShelfItem) -> Option<&str> {
        match (self, item) {
            (Self::Playlists, ShelfItem::Playlist(card)) if card.browse_id != LIKED_SONGS_ID => {
                Some(&card.browse_id)
            }
            (Self::Albums, ShelfItem::Album(card)) | (Self::Artists, ShelfItem::Artist(card)) => {
                Some(&card.browse_id)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum LibraryContent {
    Collection(Vec<HomeSection>),
    Songs(BrowsePage),
}

#[tauri::command]
#[specta::specta]
pub async fn get_library(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: LibraryKind,
) -> Result<LibraryContent, AppError> {
    signed_in_request(&app, &state, |client| async move {
        if kind == LibraryKind::Songs {
            let raw = client.browse(LIKED_SONGS_ID).await?;
            return Ok(LibraryContent::Songs(parse_browse_page_for_id(
                &raw,
                LIKED_SONGS_ID,
            )?));
        }
        Ok(LibraryContent::Collection(
            fetch_collection(&client, kind).await?,
        ))
    })
    .await
}

async fn fetch_collection(
    client: &InnerTubeClient,
    kind: LibraryKind,
) -> Result<Vec<HomeSection>, AppError> {
    let (id, params) = kind.endpoint();
    let mut page = parse_library(&client.browse_with_params(id, params).await?)?;
    let mut seen_items = HashSet::new();
    let mut seen_tokens = HashSet::new();
    let mut pending = VecDeque::new();
    let mut sections = Vec::new();

    loop {
        for mut section in page.sections {
            section.items.retain(|item| {
                kind.item_id(item)
                    .is_some_and(|id| seen_items.insert(id.to_owned()))
            });
            if !section.items.is_empty() {
                sections.push(section);
            }
        }
        for token in page.continuations {
            if seen_tokens.insert(token.clone()) {
                pending.push_back(token);
            }
        }
        let Some(token) = pending.pop_front() else {
            return Ok(sections);
        };
        checked_continuation(&token)?;
        page = parse_library_continuation(&client.browse_continuation(&token).await?)?;
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_browse_page_continuation(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<BrowsePageContinuation, AppError> {
    checked_continuation(&token)?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_browse_page_continuation(
            &client.browse_continuation(&token).await?,
        )?)
    })
    .await
}

fn checked_continuation(token: &str) -> Result<(), AppError> {
    checked_browse_value(token, 1..=4096, b"%=+/.", "Invalid continuation.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn library_ids_match_swift_requests() {
        assert_eq!(
            LibraryKind::Playlists.endpoint(),
            ("FEmusic_liked_playlists", None)
        );
        assert_eq!(
            LibraryKind::Albums.endpoint(),
            ("FEmusic_liked_albums", None)
        );
        assert_eq!(
            LibraryKind::Artists.endpoint(),
            ("FEmusic_library_corpus_artists", Some("ggMCCAU="))
        );
        assert_eq!(LibraryKind::Songs.endpoint(), ("VLLM", None));
    }

    #[test]
    fn continuation_validator_accepts_base64_and_rejects_invalid_input() {
        assert!(checked_continuation("test-_+/.%3D=").is_ok());
        assert!(checked_continuation(&"a".repeat(4096)).is_ok());
        for token in ["", "test?invalid", "test token", "가", &"a".repeat(4097)] {
            assert_eq!(
                checked_continuation(token).unwrap_err().kind,
                ErrorKind::InvalidArgument
            );
        }
    }
}
