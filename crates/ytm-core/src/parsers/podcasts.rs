use serde_json::Value;

use super::home::{
    SECTION_LIST, next_continuation, parse_feed, parse_feed_continuation, parse_item,
};
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{Episode, FeedPage, PodcastEpisodesPage, PodcastShow, ShelfItem};

pub const PODCASTS_BROWSE_ID: &str = "FEmusic_podcasts";
const TWO_COLUMN: &[&str] = &["contents", "twoColumnBrowseResultsRenderer"];
const RESPONSIVE_HEADER: &[&str] = &[
    "tabs",
    "0",
    "tabRenderer",
    "content",
    "sectionListRenderer",
    "contents",
];

/// Initial podcast discovery shelves use the same layout as Home.
pub fn parse_podcasts(value: &Value) -> Result<FeedPage, YtmError> {
    parse_feed(value)
}

/// Later discovery shelves can arrive in a section list or carousel.
pub fn parse_podcasts_continuation(value: &Value) -> Result<FeedPage, YtmError> {
    parse_feed_continuation(value)
}

pub fn parse_podcast_show(value: &Value) -> Result<PodcastShow, YtmError> {
    let two_column = at(value, TWO_COLUMN);
    let legacy = at(value, &["header", "musicDetailHeaderRenderer"]);
    let header = rows(two_column.and_then(|column| at(column, RESPONSIVE_HEADER)))
        .iter()
        .find_map(|section| section.get("musicResponsiveHeaderRenderer"));
    let text = |key| {
        header
            .and_then(|header| header.get(key))
            .and_then(runs_text)
            .or_else(|| {
                legacy
                    .and_then(|header| header.get(key))
                    .and_then(runs_text)
            })
    };
    let title = text("title").ok_or_else(|| parse_error("header.title.runs"))?;
    let secondary =
        two_column.and_then(|column| at(column, &["secondaryContents", "sectionListRenderer"]));
    let mut page = episode_sections(secondary);
    if page.episodes.is_empty() {
        let single = episode_sections(at(value, SECTION_LIST));
        page.episodes = single.episodes;
        page.continuation = page.continuation.or(single.continuation);
    }
    Ok(PodcastShow {
        title,
        author: text("subtitle"),
        description: text("description"),
        thumbnail_url: header
            .and_then(thumbnail_url)
            .or_else(|| legacy.and_then(thumbnail_url)),
        episodes: page.episodes,
        continuation: page.continuation,
    })
}

pub fn parse_podcast_episodes_continuation(value: &Value) -> Result<PodcastEpisodesPage, YtmError> {
    let shelf = at(value, &["continuationContents", "musicShelfContinuation"])
        .filter(|shelf| shelf.is_object())
        .ok_or_else(|| parse_error("continuationContents.musicShelfContinuation"))?;
    Ok(episode_shelf(shelf))
}

fn rows(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn episode_sections(list: Option<&Value>) -> PodcastEpisodesPage {
    let mut page = PodcastEpisodesPage {
        episodes: Vec::new(),
        continuation: None,
    };
    for shelf in rows(list.and_then(|list| list.get("contents")))
        .iter()
        .filter_map(|section| section.get("musicShelfRenderer"))
    {
        let next = episode_shelf(shelf);
        page.episodes.extend(next.episodes);
        page.continuation = next.continuation.or(page.continuation);
    }
    page
}

fn episode_shelf(shelf: &Value) -> PodcastEpisodesPage {
    let episodes = rows(shelf.get("contents"))
        .iter()
        .filter_map(|item| match parse_item(item)? {
            ShelfItem::Episode(episode) => Some(episode),
            ShelfItem::Song(song) if !song.video_id.is_empty() => Some(Episode {
                video_id: song.video_id,
                title: song.title,
                thumbnail_url: song.thumbnail_url,
                show_title: song.artists.first().map(|artist| artist.name.clone()),
                description: None,
                date: None,
                duration: None,
                progress: None,
                show_browse_id: None,
            }),
            _ => None,
        })
        .collect();
    PodcastEpisodesPage {
        episodes,
        continuation: next_continuation(shelf),
    }
}

pub(super) fn parse_multi_row_episode(renderer: &Value) -> Option<Episode> {
    let video_id =
        str_at(renderer, &["onTap", "watchEndpoint", "videoId"]).filter(|id| !id.is_empty())?;
    let first_text = |key| str_at(renderer, &[key, "runs", "0", "text"]).map(str::to_owned);
    let show_browse_id = rows(at(renderer, &["subtitle", "runs"]))
        .iter()
        .filter_map(|run| str_at(run, &["navigationEndpoint", "browseEndpoint", "browseId"]))
        .find(|id| id.starts_with("MPSPP"))
        .map(str::to_owned);
    let progress =
        if first_text("playedText").is_some_and(|text| text.eq_ignore_ascii_case("played")) {
            Some(1.0)
        } else {
            at(
                renderer,
                &["playbackProgress", "playbackProgressPercentage"],
            )
            .and_then(Value::as_f64)
            .map(|percent| percent.clamp(0.0, 100.0) / 100.0)
        };

    Some(Episode {
        video_id: video_id.into(),
        title: first_text("title").unwrap_or_else(|| "Unknown Episode".into()),
        description: renderer.get("description").and_then(runs_text),
        date: first_text("publishedTimeText"),
        duration: first_text("durationText"),
        thumbnail_url: thumbnail_url(renderer),
        progress,
        show_title: first_text("subtitle"),
        show_browse_id,
    })
}

fn parse_error(path: &str) -> YtmError {
    YtmError::Parse {
        endpoint: "browse:podcast".into(),
        path: path.into(),
    }
}
