use serde_json::Value;

use super::home::parse_responsive_song;
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::BrowsePage;

const TAB_CONTENT: &[&str] = &["tabs", "0", "tabRenderer", "content"];

/// Parses the first browse response for an album or playlist.
pub fn parse_browse_page(value: &Value) -> Result<BrowsePage, YtmError> {
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
        .or_else(|| at(value, &["header", "musicImmersiveHeaderRenderer"]))
        .ok_or_else(|| parse_error("header"))?;
    let title = header
        .get("title")
        .and_then(runs_text)
        .ok_or_else(|| parse_error("header.title.runs"))?;
    let thumbnail = thumbnail_url(header);
    let subtitle = header
        .get("subtitle")
        .and_then(runs_text)
        .or_else(|| header.get("straplineTextOne").and_then(runs_text));

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
    let track_shelves: Vec<_> = if playlist_shelves.peek().is_some() {
        playlist_shelves.collect()
    } else {
        shelves("musicShelfRenderer").collect()
    };
    let mut continuation = None;
    let mut songs = Vec::new();
    for shelf in track_shelves {
        let rows = array(shelf.get("contents"));
        continuation = continuation.or_else(|| shelf_continuation(shelf, rows).map(str::to_owned));
        songs.extend(rows.iter().filter_map(|row| {
            let mut song = parse_responsive_song(row.get("musicResponsiveListItemRenderer")?)?;
            if song.video_id.is_empty() {
                return None;
            }
            if song.thumbnail_url.is_none() {
                song.thumbnail_url.clone_from(&thumbnail);
            }
            Some(song)
        }));
    }

    Ok(BrowsePage {
        title,
        subtitle,
        second_subtitle: header.get("secondSubtitle").and_then(runs_text),
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

fn shelf_continuation<'a>(shelf: &'a Value, rows: &'a [Value]) -> Option<&'a str> {
    str_at(
        shelf,
        &["continuations", "0", "nextContinuationData", "continuation"],
    )
    .or_else(|| {
        str_at(
            rows.last()?,
            &[
                "continuationItemRenderer",
                "continuationEndpoint",
                "continuationCommand",
                "token",
            ],
        )
    })
}

fn parse_error(path: &str) -> YtmError {
    YtmError::Parse {
        endpoint: "browse".into(),
        path: path.into(),
    }
}
