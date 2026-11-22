use serde_json::Value;

use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{ArtistRef, BrowseCard, HomePage, HomeSection, ShelfItem, Song};

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

fn parse_section(section: &Value) -> Option<HomeSection> {
    let Some(shelf) = section.get("musicCarouselShelfRenderer") else {
        let kind = section
            .as_object()
            .and_then(|o| o.keys().next())
            .map_or("?", String::as_str);
        tracing::debug!(kind, "skipping unsupported home section");
        return None;
    };
    let title = at(
        shelf,
        &["header", "musicCarouselShelfBasicHeaderRenderer", "title"],
    )
    .and_then(runs_text)
    .unwrap_or_else(|| "Unknown Section".into());
    let items: Vec<ShelfItem> = shelf
        .get("contents")?
        .as_array()?
        .iter()
        .filter_map(parse_item)
        .collect();
    (!items.is_empty()).then_some(HomeSection { title, items })
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
        .or_else(|| str_at(renderer, PLAY_BUTTON_VIDEO_ID));
    if let Some(video_id) = video_id {
        return Some(ShelfItem::Song(Song {
            video_id: video_id.into(),
            title,
            artists: artists_from_runs(flex_column(renderer, 1)),
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

fn page_type(browse: &Value) -> Option<&str> {
    str_at(
        browse,
        &[
            "browseEndpointContextSupportedConfigs",
            "browseEndpointContextMusicConfig",
            "pageType",
        ],
    )
}

/// pageType first, then browseId prefix — same order as the Swift HomeResponseParser.
fn classify(page_type: Option<&str>, card: BrowseCard) -> Option<ShelfItem> {
    match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM") => return Some(ShelfItem::Album(card)),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(ShelfItem::Playlist(card)),
        Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
            return Some(ShelfItem::Artist(card));
        }
        _ => {}
    }
    let id = card.browse_id.as_str();
    if id.starts_with("MPRE") || id.starts_with("OLAK") {
        Some(ShelfItem::Album(card))
    } else if ["VL", "PL", "RD"].iter().any(|p| id.starts_with(p)) {
        Some(ShelfItem::Playlist(card))
    } else if id.starts_with("UC") {
        Some(ShelfItem::Artist(card))
    } else {
        None
    }
}

/// Runs linking to a `UC…` channel are artists. Falls back to the first run's text.
fn artists_from_runs(text: Option<&Value>) -> Vec<ArtistRef> {
    let Some(runs) = text.and_then(|t| t.get("runs")).and_then(Value::as_array) else {
        return Vec::new();
    };
    let linked: Vec<ArtistRef> = runs
        .iter()
        .filter_map(|run| {
            let id = str_at(run, &["navigationEndpoint", "browseEndpoint", "browseId"])?;
            if !id.starts_with("UC") {
                return None;
            }
            let name = run.get("text")?.as_str()?;
            Some(ArtistRef {
                name: name.into(),
                browse_id: Some(id.into()),
            })
        })
        .collect();
    if !linked.is_empty() {
        return linked;
    }
    runs.first()
        .and_then(|run| run.get("text")?.as_str())
        .map(|name| {
            vec![ArtistRef {
                name: name.into(),
                browse_id: None,
            }]
        })
        .unwrap_or_default()
}
