use serde_json::Value;

use super::helpers::{
    artists_from_flex_column, artists_from_runs, is_artist_page_type, is_navigable_artist_id,
    page_type,
};
use super::podcasts::parse_multi_row_episode;
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{BrowseCard, Chip, FeedPage, HomePage, HomeSection, ShelfItem, Song};

pub(super) const SECTION_LIST: &[&str] = &[
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
    let page = parse_feed_at(response, SECTION_LIST, "browse:FEmusic_home")?;
    Ok(HomePage {
        sections: page.sections,
        continuation: page.continuation,
    })
}

pub fn parse_feed(response: &Value) -> Result<FeedPage, YtmError> {
    parse_feed_at(response, SECTION_LIST, "browse")
}

pub fn parse_feed_continuation(response: &Value) -> Result<FeedPage, YtmError> {
    let path = &["continuationContents", "sectionListContinuation"];
    if let Some(shelf) = at(
        response,
        &["continuationContents", "musicShelfContinuation"],
    ) && at(response, path).is_none()
    {
        return Ok(FeedPage {
            sections: parse_music_shelf(shelf).into_iter().collect(),
            continuation: next_continuation(shelf),
            chips: Vec::new(),
        });
    }
    let carousel = at(
        response,
        &["continuationContents", "musicCarouselShelfContinuation"],
    );
    let mut page = if carousel.is_some() && at(response, path).is_none() {
        FeedPage {
            sections: Vec::new(),
            continuation: None,
            chips: Vec::new(),
        }
    } else {
        parse_feed_at(response, path, "browse")?
    };
    if let Some(carousel) = carousel {
        let title = carousel_title(carousel).unwrap_or_else(|| "More".into());
        page.sections
            .extend(shelf_section(title, carousel.get("contents")));
        page.continuation = page.continuation.or_else(|| next_continuation(carousel));
    }
    Ok(page)
}

fn parse_feed_at(response: &Value, path: &[&str], endpoint: &str) -> Result<FeedPage, YtmError> {
    let list = at(response, path)
        .filter(|list| list.is_object())
        .ok_or_else(|| YtmError::Parse {
            endpoint: endpoint.into(),
            path: path.join("."),
        })?;
    let sections = list
        .get("contents")
        .and_then(Value::as_array)
        .map(|contents| contents.iter().filter_map(parse_section).collect())
        .unwrap_or_default();
    Ok(FeedPage {
        sections,
        continuation: next_continuation(list),
        chips: parse_chips(list),
    })
}

pub(super) fn next_continuation(list: &Value) -> Option<String> {
    str_at(
        list,
        &["continuations", "0", "nextContinuationData", "continuation"],
    )
    .filter(|token| !token.is_empty())
    .map(str::to_owned)
}

fn parse_chips(list: &Value) -> Vec<Chip> {
    let Some(header) = list.get("header") else {
        return Vec::new();
    };
    let cloud = header
        .get("chipCloudRenderer")
        .or_else(|| header.get("musicChipCloudRenderer"));
    cloud
        .and_then(|cloud| cloud.get("chips"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|chip| {
            let chip = chip.get("chipCloudChipRenderer")?;
            let params = str_at(chip, &["navigationEndpoint", "browseEndpoint", "params"])
                .or_else(|| str_at(chip, &["onTap", "browseEndpoint", "params"]))?;
            if params.is_empty() {
                return None;
            }
            Some(Chip {
                title: runs_text(chip.get("text")?)?,
                params: params.into(),
            })
        })
        .collect()
}

/// Parses shelves shared by the music feeds.
pub(super) fn parse_section(section: &Value) -> Option<HomeSection> {
    if let Some(shelf) = section.get("musicCarouselShelfRenderer") {
        let title = carousel_title(shelf).unwrap_or_else(|| "Unknown Section".into());
        return shelf_section(title, shelf.get("contents"));
    }
    if let Some(shelf) = section
        .get("musicShelfRenderer")
        .or_else(|| section.get("musicPlaylistShelfRenderer"))
    {
        return parse_music_shelf(shelf);
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
        "skipping unsupported feed section"
    );
    None
}

fn parse_music_shelf(shelf: &Value) -> Option<HomeSection> {
    let title = shelf
        .get("title")
        .and_then(runs_text)
        .unwrap_or_else(|| "Unknown Section".into());
    shelf_section(title, shelf.get("contents"))
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
                tracing::debug!(kind = first_key(item), "skipping unparsed feed shelf item");
            }
            parsed
        })
        .collect();
    (!items.is_empty()).then_some(HomeSection {
        title,
        items,
        more: None,
    })
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
    if let Some(renderer) = item.get("musicMultiRowListItemRenderer") {
        return parse_multi_row_episode(renderer).map(ShelfItem::Episode);
    }
    let button = item.get("musicNavigationButtonRenderer")?;
    let browse = at(button, &["clickCommand", "browseEndpoint"])?;
    Some(ShelfItem::Category {
        title: runs_text(button.get("buttonText")?)?,
        browse_id: str_at(browse, &["browseId"])?.into(),
        params: str_at(browse, &["params"]).map(str::to_owned),
        color: at(button, &["solid", "leftStripeColor"])
            .and_then(Value::as_i64)
            .map(|argb| format!("#{:06x}", argb & 0xff_ffff)),
    })
}

fn parse_watch_item(renderer: &Value, endpoint_key: &str) -> Option<Song> {
    let video_id = str_at(renderer, &[endpoint_key, "watchEndpoint", "videoId"])
        .filter(|id| !id.is_empty())?;
    Some(Song {
        video_id: video_id.into(),
        title: renderer.get("title").and_then(runs_text)?,
        artists: artists_from_runs(renderer.get("subtitle")),
        thumbnail_url: thumbnail_url(renderer),
    })
}

fn parse_two_row(renderer: &Value) -> Option<ShelfItem> {
    if let Some(song) = parse_watch_item(renderer, "navigationEndpoint") {
        return Some(ShelfItem::Song(song));
    }
    let title = renderer.get("title").and_then(runs_text)?;
    let thumbnail = thumbnail_url(renderer);
    let nav = renderer.get("navigationEndpoint")?;
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
    if let Some(song) = parse_responsive_song(renderer) {
        return Some(ShelfItem::Song(song));
    }
    let browse = at(renderer, &["navigationEndpoint", "browseEndpoint"])?;
    let card = BrowseCard {
        browse_id: browse.get("browseId")?.as_str()?.into(),
        title: flex_column(renderer, 0).and_then(runs_text)?,
        subtitle: flex_column(renderer, 1).and_then(runs_text),
        thumbnail_url: thumbnail_url(renderer),
    };
    classify(page_type(browse), card)
}

pub(super) fn parse_responsive_song(renderer: &Value) -> Option<Song> {
    let video_id = str_at(renderer, &["playlistItemData", "videoId"])
        .or_else(|| {
            str_at(
                renderer,
                &["navigationEndpoint", "watchEndpoint", "videoId"],
            )
        })
        .or_else(|| str_at(renderer, PLAY_BUTTON_VIDEO_ID))?;
    Some(Song {
        video_id: video_id.into(),
        title: flex_column(renderer, 0).and_then(runs_text)?,
        artists: artists_from_flex_column(flex_column(renderer, 1)),
        thumbnail_url: thumbnail_url(renderer),
    })
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
        Some("MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE") => return Some(ShelfItem::Podcast(card)),
        Some("MUSIC_PAGE_TYPE_ALBUM") => return Some(ShelfItem::Album(card)),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(ShelfItem::Playlist(card)),
        page_type if is_artist_page_type(page_type) => {
            return Some(ShelfItem::Artist(card));
        }
        _ => {}
    }
    let id = card.browse_id.as_str();
    if id.starts_with("MPSPP") {
        Some(ShelfItem::Podcast(card))
    } else if id.starts_with("MPRE") || id.starts_with("OLAK") {
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
