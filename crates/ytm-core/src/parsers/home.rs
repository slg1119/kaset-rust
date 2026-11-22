use serde_json::Value;

use super::helpers::{
    artists_from_flex_column, artists_from_runs, is_artist_page_type, is_navigable_artist_id,
    page_type,
};
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{BrowseCard, HomePage, HomeSection, ShelfItem, Song};

const SECTION_LIST: &[&str] = &[
    "contents",
    "singleColumnBrowseResultsRenderer",
    "tabs",
    "0",
    "tabRenderer",
    "content",
    "sectionListRenderer",
];

const PLAY_BUTTON_VIDEO_ID: &[&str] = &[
    "overlay",
    "musicItemThumbnailOverlayRenderer",
    "content",
    "musicPlayButtonRenderer",
    "playNavigationEndpoint",
    "watchEndpoint",
    "videoId",
];

pub fn parse_home(response: &Value) -> Result<HomePage, YtmError> {
    let list = at(response, SECTION_LIST).ok_or_else(|| YtmError::Parse {
        endpoint: "browse:FEmusic_home".into(),
        path: SECTION_LIST.join("."),
    })?;
    let sections = list
        .get("contents")
        .and_then(Value::as_array)
        .map(|contents| contents.iter().filter_map(parse_section).collect())
        .unwrap_or_default();
    let continuation = str_at(
        list,
        &["continuations", "0", "nextContinuationData", "continuation"],
    )
    .map(str::to_owned);
    Ok(HomePage {
        sections,
        continuation,
    })
}

/// One home section. Shelf types follow the Swift `HomeResponseParser`:
/// carousels, vertical shelves, card shelves, immersive carousels, grids, and
/// item sections that wrap one of them. Mood and genre buttons are skipped.
fn parse_section(section: &Value) -> Option<HomeSection> {
    if let Some(shelf) = section.get("musicCarouselShelfRenderer") {
        let title = carousel_title(shelf).unwrap_or_else(|| "Unknown Section".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicShelfRenderer") {
        let title = shelf
            .get("title")
            .and_then(runs_text)
            .unwrap_or_else(|| "Unknown Section".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicCardShelfRenderer") {
        let title = at(
            shelf,
            &["header", "musicCardShelfHeaderBasicRenderer", "title"],
        )
        .and_then(runs_text)
        .unwrap_or_else(|| "Featured".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section.get("musicImmersiveCarouselShelfRenderer") {
        let title = carousel_title(shelf).unwrap_or_else(|| "Featured".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(grid) = section.get("gridRenderer") {
        let title = at(grid, &["header", "gridHeaderRenderer", "title"])
            .and_then(runs_text)
            .unwrap_or_else(|| "Charts".into());
        return shelf_section(title, grid.get("items"));
    }
    if let Some(wrapper) = section.get("itemSectionRenderer") {
        return wrapper
            .get("contents")?
            .as_array()?
            .iter()
            .find_map(parse_section);
    }
    tracing::debug!(
        kind = first_key(section),
        "skipping unsupported home section"
    );
    None
}

fn carousel_title(shelf: &Value) -> Option<String> {
    at(
        shelf,
        &["header", "musicCarouselShelfBasicHeaderRenderer", "title"],
    )
    .and_then(runs_text)
}

/// A section from `contents`; `None` when no item parses.
fn shelf_section(title: String, contents: Option<&Value>) -> Option<HomeSection> {
    let items: Vec<ShelfItem> = contents?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let parsed = parse_item(item);
            if parsed.is_none() {
                tracing::debug!(kind = first_key(item), "skipping unparsed home shelf item");
            }
            parsed
        })
        .collect();
    (!items.is_empty()).then_some(HomeSection { title, items })
}

fn first_key(value: &Value) -> &str {
    value
        .as_object()
        .and_then(|object| object.keys().next())
        .map_or("?", String::as_str)
}

pub(crate) fn parse_item(item: &Value) -> Option<ShelfItem> {
    if let Some(renderer) = item.get("musicTwoRowItemRenderer") {
        return parse_two_row(renderer);
    }
    if let Some(renderer) = item.get("musicResponsiveListItemRenderer") {
        return parse_responsive(renderer);
    }
    None
}

fn parse_two_row(renderer: &Value) -> Option<ShelfItem> {
    let title = renderer.get("title").and_then(runs_text)?;
    let thumbnail = thumbnail_url(renderer);
    let nav = renderer.get("navigationEndpoint")?;
    if let Some(video_id) = str_at(nav, &["watchEndpoint", "videoId"]) {
        return Some(ShelfItem::Song(Song {
            video_id: video_id.into(),
            title,
            artists: artists_from_runs(renderer.get("subtitle")),
            thumbnail_url: thumbnail,
        }));
    }
    let browse = nav.get("browseEndpoint")?;
    let card = BrowseCard {
        browse_id: browse.get("browseId")?.as_str()?.into(),
        title,
        subtitle: renderer.get("subtitle").and_then(runs_text),
        thumbnail_url: thumbnail,
    };
    classify(page_type(browse), card)
}

fn parse_responsive(renderer: &Value) -> Option<ShelfItem> {
    let title = flex_column(renderer, 0).and_then(runs_text)?;
    let thumbnail = thumbnail_url(renderer);
    let video_id = str_at(renderer, &["playlistItemData", "videoId"])
        .or_else(|| {
            str_at(
                renderer,
                &["navigationEndpoint", "watchEndpoint", "videoId"],
            )
        })
        .or_else(|| str_at(renderer, PLAY_BUTTON_VIDEO_ID));
    if let Some(video_id) = video_id {
        return Some(ShelfItem::Song(Song {
            video_id: video_id.into(),
            title,
            artists: artists_from_flex_column(flex_column(renderer, 1)),
            thumbnail_url: thumbnail,
        }));
    }
    let browse = at(renderer, &["navigationEndpoint", "browseEndpoint"])?;
    let card = BrowseCard {
        browse_id: browse.get("browseId")?.as_str()?.into(),
        title,
        subtitle: flex_column(renderer, 1).and_then(runs_text),
        thumbnail_url: thumbnail,
    };
    classify(page_type(browse), card)
}

fn flex_column(renderer: &Value, index: usize) -> Option<&Value> {
    renderer
        .get("flexColumns")?
        .get(index)?
        .get("musicResponsiveListItemFlexColumnRenderer")?
        .get("text")
}

/// pageType first, then browseId prefix — same order as the Swift HomeResponseParser.
fn classify(page_type: Option<&str>, card: BrowseCard) -> Option<ShelfItem> {
    match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM") => return Some(ShelfItem::Album(card)),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(ShelfItem::Playlist(card)),
        page_type if is_artist_page_type(page_type) => {
            return Some(ShelfItem::Artist(card));
        }
        _ => {}
    }
    let id = card.browse_id.as_str();
    if id.starts_with("MPRE") || id.starts_with("OLAK") {
        Some(ShelfItem::Album(card))
    } else if ["VL", "PL", "RD"].iter().any(|p| id.starts_with(p)) {
        Some(ShelfItem::Playlist(card))
    } else if is_navigable_artist_id(id) {
        Some(ShelfItem::Artist(card))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn two_row_album_reads_thumbnail_renderer_artwork() {
        let item = json!({
            "musicTwoRowItemRenderer": {
                "title": {"runs": [{"text": "Test Album"}]},
                "navigationEndpoint": {"browseEndpoint": {"browseId": "MPREtest"}},
                "thumbnailRenderer": {
                    "musicThumbnailRenderer": {
                        "thumbnail": {
                            "thumbnails": [{"url": "https://lh3.googleusercontent.com/test-art"}]
                        }
                    }
                }
            }
        });

        let Some(ShelfItem::Album(card)) = parse_item(&item) else {
            panic!("expected an album card");
        };
        assert_eq!(
            card.thumbnail_url.as_deref(),
            Some("https://lh3.googleusercontent.com/test-art")
        );
    }
}
