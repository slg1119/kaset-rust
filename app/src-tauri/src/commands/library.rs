use std::collections::{HashSet, VecDeque};
use std::future::Future;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ytm_core::models::{BrowsePage, BrowsePageContinuation, HomeSection, LibraryPage, ShelfItem};
use ytm_core::parsers::browse_page::{parse_browse_page_continuation, parse_browse_page_for_id};
use ytm_core::parsers::library::{parse_library, parse_library_continuation};

use super::{checked_token, on_error, signed_in_request};
use crate::error::AppError;
use crate::state::AppState;

const LIKED_SONGS_ID: &str = "VLLM";
const MAX_COLLECTION_PAGES: usize = 50;

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
    let (app, state) = (&app, state.inner());
    let epoch = state.session.epoch();
    signed_in_request(app, state, |client| async move {
        if kind == LibraryKind::Songs {
            let raw = client.browse(LIKED_SONGS_ID).await?;
            return Ok(LibraryContent::Songs(parse_browse_page_for_id(
                &raw,
                LIKED_SONGS_ID,
            )?));
        }
        let (id, params) = kind.endpoint();
        let first = parse_library(&client.browse_with_params(id, params).await?)?;
        let client = &client;
        let sections = fetch_collection(kind, first, |token| async move {
            if state.session.epoch() != epoch {
                return Err(AppError::not_signed_in());
            }
            match client.browse_continuation(&token).await {
                Ok(raw) => Ok(parse_library_continuation(&raw)?),
                Err(error) => Err(on_error(app, epoch, error.into()).await),
            }
        })
        .await;
        Ok(LibraryContent::Collection(sections))
    })
    .await
}

async fn fetch_collection<Fut: Future<Output = Result<LibraryPage, AppError>>>(
    kind: LibraryKind,
    mut page: LibraryPage,
    mut fetch: impl FnMut(String) -> Fut,
) -> Vec<HomeSection> {
    let mut seen_items = HashSet::new();
    let mut seen_tokens = HashSet::new();
    let mut pending = VecDeque::new();
    let mut sections = Vec::new();

    for page_number in 1..=MAX_COLLECTION_PAGES {
        for mut section in page.sections {
            section.items.retain(|item| {
                kind.item_id(item)
                    .is_some_and(|id| seen_items.insert(id.to_owned()))
            });
            if !section.items.is_empty() {
                sections.push(section);
            }
        }
        if page_number == MAX_COLLECTION_PAGES {
            break;
        }
        for token in page.continuations {
            if seen_tokens.insert(token.clone()) {
                pending.push_back(token);
            }
        }
        let Some(token) = pending.pop_front() else {
            break;
        };
        match fetch(token).await {
            Ok(next) => page = next,
            Err(error) => {
                tracing::warn!(?kind, error_kind = ?error.kind, page_number = page_number + 1, "library continuation failed; keeping loaded pages");
                break;
            }
        }
    }
    sections
}

#[tauri::command]
#[specta::specta]
pub async fn get_browse_page_continuation(
    app: AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<BrowsePageContinuation, AppError> {
    checked_token(&token, "Invalid continuation.")?;
    signed_in_request(&app, &state, |client| async move {
        Ok(parse_browse_page_continuation(
            &client.browse_continuation(&token).await?,
        )?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::future::ready;
    use ytm_core::models::BrowseCard;

    fn library_page(ids: &[&str], tokens: &[&str]) -> LibraryPage {
        LibraryPage {
            sections: vec![HomeSection {
                title: "Test albums".into(),
                items: ids
                    .iter()
                    .map(|id| {
                        ShelfItem::Album(BrowseCard {
                            browse_id: (*id).into(),
                            title: (*id).into(),
                            subtitle: None,
                            thumbnail_url: None,
                        })
                    })
                    .collect(),
                more: None,
            }],
            continuations: tokens.iter().map(|token| (*token).into()).collect(),
        }
    }

    fn album_ids(sections: &[HomeSection]) -> Vec<&str> {
        sections
            .iter()
            .flat_map(|section| &section.items)
            .map(|item| LibraryKind::Albums.item_id(item).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn library_paging_deduplicates_items_and_tokens_and_passes_opaque_cursors() {
        let first = library_page(&["test-a", "test-a"], &["test-continuation?", "test-other"]);
        let mut tokens = Vec::new();
        let sections = fetch_collection(LibraryKind::Albums, first, |token| {
            let page = match token.as_str() {
                "test-continuation?" => {
                    library_page(&["test-a", "test-b"], &["test-other", "test-continuation?"])
                }
                "test-other" => library_page(&["test-b", "test-c"], &[]),
                _ => panic!("unexpected continuation"),
            };
            tokens.push(token);
            ready(Ok(page))
        })
        .await;
        assert_eq!(tokens, ["test-continuation?", "test-other"]);
        assert_eq!(album_ids(&sections), ["test-a", "test-b", "test-c"]);
    }

    #[tokio::test]
    async fn library_paging_stops_after_fifty_pages_including_the_first() {
        let first = library_page(&["test-0"], &["test-continuation-1"]);
        let mut fetched = 0;
        let sections = fetch_collection(LibraryKind::Albums, first, |_| {
            fetched += 1;
            let item = format!("test-{fetched}");
            let next = format!("test-continuation-{}", fetched + 1);
            let more = [next.as_str()];
            ready(Ok(library_page(
                &[&item],
                if fetched < 55 { &more } else { &[] },
            )))
        })
        .await;
        assert_eq!(fetched, 49);
        let ids = album_ids(&sections);
        assert_eq!(ids.len(), 50);
        assert_eq!(ids.last(), Some(&"test-49"));
    }

    #[tokio::test]
    async fn library_paging_keeps_loaded_sections_when_a_later_page_fails() {
        let first = library_page(&["test-a"], &["test-first"]);
        let sections = fetch_collection(LibraryKind::Albums, first, |token| {
            ready(match token.as_str() {
                "test-first" => Ok(library_page(&["test-b"], &["test-fail", "test-unused"])),
                "test-fail" => Err(AppError::new(ErrorKind::Network, "test failure")),
                _ => panic!("requested another page after a failure"),
            })
        })
        .await;
        assert_eq!(album_ids(&sections), ["test-a", "test-b"]);
    }

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
        assert!(checked_token("test-_+/.%3D=", "Invalid continuation.").is_ok());
        assert!(checked_token(&"a".repeat(4096), "Invalid continuation.").is_ok());
        for token in ["", "test?invalid", "test token", "가", &"a".repeat(4097)] {
            assert_eq!(
                checked_token(token, "Invalid continuation.")
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidArgument
            );
        }
    }
}
