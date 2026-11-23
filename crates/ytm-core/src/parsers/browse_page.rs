use serde_json::Value;

use super::account_actions::{playlist_actions, playlist_set_video_id};
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
        set_video_ids,
    } = parse_track_shelves(track_shelves);
    for song in &mut songs {
        if song.thumbnail_url.is_none() {
            song.thumbnail_url.clone_from(&thumbnail);
        }
    }

    Ok(BrowsePage {
        playlist: (!is_album
            && browse_id.is_none_or(|id| {
                let id = id.strip_prefix("VL").unwrap_or(id);
                !matches!(id, "LM" | "SE") && !id.starts_with("FE") && !id.starts_with("RD")
            }))
        .then(|| playlist_actions(value, header)),
        set_video_ids,
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
            && shelf.get("contents").is_some_and(Value::is_array)
        {
            return Ok(parse_track_shelves([shelf]));
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
        return Ok(parse_rows(action, rows));
    }
    Err(parse_error("playlist continuation"))
}

fn parse_track_shelves<'a>(shelves: impl IntoIterator<Item = &'a Value>) -> BrowsePageContinuation {
    let mut page = BrowsePageContinuation {
        songs: Vec::new(),
        set_video_ids: None,
        continuation: None,
    };
    let mut ids = Vec::new();
    for shelf in shelves {
        let rows = array(shelf.get("contents"));
        let batch = parse_rows(shelf, rows);
        page.continuation = page.continuation.or(batch.continuation);
        ids.extend(
            batch
                .set_video_ids
                .unwrap_or_else(|| vec![None; batch.songs.len()]),
        );
        page.songs.extend(batch.songs);
    }
    page.set_video_ids = ids.iter().any(Option::is_some).then_some(ids);
    page
}

fn parse_rows(shelf: &Value, rows: &[Value]) -> BrowsePageContinuation {
    let (songs, ids): (Vec<Song>, Vec<_>) = rows
        .iter()
        .filter_map(|row| {
            let renderer = row.get("musicResponsiveListItemRenderer")?;
            let song = parse_responsive_song(renderer)?;
            (!song.video_id.is_empty()).then(|| (song, playlist_set_video_id(renderer)))
        })
        .unzip();
    BrowsePageContinuation {
        songs,
        set_video_ids: ids.iter().any(Option::is_some).then_some(ids),
        continuation: shelf_continuation(shelf, rows).map(str::to_owned),
    }
}

fn parse_error(path: &str) -> YtmError {
    YtmError::Parse {
        endpoint: "browse".into(),
        path: path.into(),
    }
}
