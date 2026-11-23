use serde_json::Value;

use super::helpers::{append_actions, shelf_continuation};
use super::home::parse_responsive_song;
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{BrowsePage, BrowsePageContinuation, Song};

const TAB_CONTENT: &[&str] = &["tabs", "0", "tabRenderer", "content"];

/// Parses the first browse response for an album or playlist.
pub fn parse_browse_page(value: &Value) -> Result<BrowsePage, YtmError> {
    parse_page(value, None)
}

/// The request ID disambiguates album shelves when a header has no type label.
pub fn parse_browse_page_for_id(value: &Value, browse_id: &str) -> Result<BrowsePage, YtmError> {
    parse_page(value, Some(browse_id))
}

fn parse_page(value: &Value, browse_id: Option<&str>) -> Result<BrowsePage, YtmError> {
    let two = at(value, &["contents", "twoColumnBrowseResultsRenderer"]);
    let primary = sections(two.and_then(|renderer| at(renderer, TAB_CONTENT)));
    let secondary = sections(two.and_then(|renderer| renderer.get("secondaryContents")));
    let single = at(value, &["contents", "singleColumnBrowseResultsRenderer"]);
    let single = sections(
        single
            .and_then(|renderer| at(renderer, TAB_CONTENT))
            .or_else(|| value.get("contents")),
    );
    let header = primary
        .iter()
        .chain(single)
        .chain(secondary)
        .find_map(|section| section.get("musicResponsiveHeaderRenderer"))
        .or_else(|| at(value, &["header", "musicDetailHeaderRenderer"]))
        .or_else(|| {
            at(
                value,
                &[
                    "header",
                    "musicEditablePlaylistDetailHeaderRenderer",
                    "header",
                    "musicDetailHeaderRenderer",
                ],
            )
        })
        .or_else(|| at(value, &["header", "musicVisualHeaderRenderer"]))
        .or_else(|| at(value, &["header", "musicImmersiveHeaderRenderer"]));
    let is_album = browse_id.map_or_else(
        || {
            matches!(
                header.and_then(|value| str_at(value, &["subtitle", "runs", "0", "text"])),
                Some("Album" | "Single" | "EP")
            )
        },
        |id| id.starts_with("MPRE") || id.starts_with("VLOLAK"),
    );

    // Playlist shelves are authoritative even when empty. Generic music
    // shelves alongside them can contain suggestions.
    let shelves = |key| {
        secondary
            .iter()
            .chain(single)
            .chain(primary)
            .filter_map(move |section| section.get(key))
    };
    let mut playlist_shelves = shelves("musicPlaylistShelfRenderer").peekable();
    let has_playlist_shelf = playlist_shelves.peek().is_some();
    let title = match header {
        Some(header) => header
            .get("title")
            .and_then(runs_text)
            .ok_or_else(|| parse_error("header.title.runs"))?,
        // Album playlist aliases return a shelf without a header. The route
        // already carries the album's display title.
        None if has_playlist_shelf && browse_id.is_some_and(|id| id.starts_with("VLOLAK")) => {
            String::new()
        }
        None => return Err(parse_error("header")),
    };
    let thumbnail = header.and_then(thumbnail_url);
    let subtitle = header.and_then(|header| {
        header
            .get("straplineTextOne")
            .and_then(runs_text)
            .or_else(|| header.get("subtitle").and_then(runs_text))
    });

    let track_shelves: Vec<_> = if has_playlist_shelf {
        playlist_shelves.collect()
    } else if is_album {
        shelves("musicShelfRenderer").collect()
    } else {
        Vec::new()
    };
    let BrowsePageContinuation {
        mut songs,
        continuation,
    } = parse_track_shelves(track_shelves);
    for song in &mut songs {
        if song.thumbnail_url.is_none() {
            song.thumbnail_url.clone_from(&thumbnail);
        }
    }

    Ok(BrowsePage {
        title,
        subtitle,
        second_subtitle: header
            .and_then(|header| header.get("secondSubtitle"))
            .and_then(runs_text),
        thumbnail_url: thumbnail,
        songs,
        continuation,
    })
}

fn sections(container: Option<&Value>) -> &[Value] {
    array(container.and_then(|value| at(value, &["sectionListRenderer", "contents"])))
}

fn array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// Parses the next batch of tracks independently of the first-page header.
pub fn parse_browse_page_continuation(value: &Value) -> Result<BrowsePageContinuation, YtmError> {
    for key in ["musicPlaylistShelfContinuation", "musicShelfContinuation"] {
        if let Some(shelf) = at(value, &["continuationContents", key])
            && let Some(rows) = shelf.get("contents").and_then(Value::as_array)
        {
            return Ok(BrowsePageContinuation {
                songs: parse_tracks(rows).collect(),
                continuation: shelf_continuation(shelf, rows).map(str::to_owned),
            });
        }
    }
    if let Some(rows) = at(
        value,
        &[
            "continuationContents",
            "sectionListContinuation",
            "contents",
        ],
    )
    .and_then(Value::as_array)
    {
        let mut playlists = rows
            .iter()
            .filter_map(|row| row.get("musicPlaylistShelfRenderer"))
            .peekable();
        if playlists.peek().is_some() {
            return Ok(parse_track_shelves(playlists));
        }
    }
    if let Some((action, rows)) = append_actions(value).next() {
        return Ok(BrowsePageContinuation {
            songs: parse_tracks(rows).collect(),
            continuation: shelf_continuation(action, rows).map(str::to_owned),
        });
    }
    Err(parse_error("playlist continuation"))
}

fn parse_track_shelves<'a>(shelves: impl IntoIterator<Item = &'a Value>) -> BrowsePageContinuation {
    let mut page = BrowsePageContinuation {
        songs: Vec::new(),
        continuation: None,
    };
    for shelf in shelves {
        let rows = array(shelf.get("contents"));
        page.continuation = page
            .continuation
            .or_else(|| shelf_continuation(shelf, rows).map(str::to_owned));
        page.songs.extend(parse_tracks(rows));
    }
    page
}

fn parse_tracks(rows: &[Value]) -> impl Iterator<Item = Song> {
    rows.iter().filter_map(|row| {
        let song = parse_responsive_song(row.get("musicResponsiveListItemRenderer")?)?;
        (!song.video_id.is_empty()).then_some(song)
    })
}

fn parse_error(path: &str) -> YtmError {
    YtmError::Parse {
        endpoint: "browse".into(),
        path: path.into(),
    }
}
