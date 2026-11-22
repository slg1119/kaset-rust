use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde_json::Value;

use crate::json::{at, runs_text, str_at};
use crate::models::{LyricLine, Lyrics};

pub fn parse_lyrics_browse_id(next: &Value) -> Option<&str> {
    at(
        next,
        &[
            "contents",
            "singleColumnMusicWatchNextResultsRenderer",
            "tabbedRenderer",
            "watchNextTabbedResultsRenderer",
            "tabs",
        ],
    )?
    .as_array()?
    .iter()
    .filter_map(|tab| {
        str_at(
            tab,
            &["tabRenderer", "endpoint", "browseEndpoint", "browseId"],
        )
    })
    .find(|id| id.starts_with("MPLYt"))
}

pub fn parse_lyrics(value: &Value) -> Lyrics {
    let shelf = at(value, &["contents", "sectionListRenderer", "contents"])
        .and_then(Value::as_array)
        .and_then(|sections| {
            sections.iter().find_map(|section| {
                section
                    .get("musicDescriptionShelfRenderer")
                    .filter(|renderer| renderer.is_object())
            })
        });
    let Some((shelf, text)) =
        shelf.and_then(|shelf| Some((shelf, runs_text(shelf.get("description")?)?)))
    else {
        return Lyrics::default();
    };
    Lyrics {
        text,
        source: shelf.get("footer").and_then(runs_text),
    }
}

pub fn parse_timed_lyrics(value: &Value) -> Option<Vec<LyricLine>> {
    let mut lines: Vec<_> = timed_model(value)?
        .get("lyricsData")?
        .as_array()?
        .iter()
        .filter_map(|row| {
            let start_ms: u32 = str_at(row, &["startTimeMs"])?.parse().ok()?;
            let end_ms = str_at(row, &["durationMs"])
                .and_then(|duration| duration.parse().ok())
                .filter(|duration| *duration > 0)
                .and_then(|duration| start_ms.checked_add(duration));
            Some(LyricLine {
                start_ms,
                end_ms,
                text: str_at(row, &["lyricLine"])?.into(),
            })
        })
        .collect();
    lines.sort_by_key(|line| line.start_ms);
    (!lines.is_empty()).then_some(lines)
}

fn timed_model(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(fields) => fields
            .get("timedLyricsModel")
            .filter(|model| model.is_object())
            .or_else(|| fields.values().find_map(timed_model)),
        Value::Array(values) => values.iter().find_map(timed_model),
        _ => None,
    }
}

static LRC_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([0-9]{2,}):([0-9]{2})\.([0-9]{2,3})\]").unwrap());
static LRC_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[0-9]{2,}:[0-9]{2}\.[0-9]{2,3}>").unwrap());

/// Port of Swift's LRC line timing. Enhanced word tags are stripped from the
/// displayed text because the panel synchronizes by line.
pub fn parse_lrc(raw: &str) -> Option<Vec<LyricLine>> {
    let mut lines = Vec::new();
    let mut offset_ms = 0i64;
    for raw_line in raw.lines() {
        if let Some(offset) = raw_line
            .strip_prefix("[offset:")
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(value, _)| value.trim().parse().ok())
        {
            offset_ms = offset;
        }

        let mut starts = LRC_LINE
            .captures_iter(raw_line)
            .filter_map(|stamp| {
                let ms = lrc_timestamp(&stamp)?.saturating_sub(offset_ms);
                u32::try_from(ms.max(0)).ok()
            })
            .peekable();
        if starts.peek().is_none() {
            continue;
        }
        let without_line_tags = LRC_LINE.replace_all(raw_line, "");
        let text = LRC_WORD.replace_all(&without_line_tags, "");
        lines.extend(starts.map(|start_ms| LyricLine {
            start_ms,
            end_ms: None,
            text: text.trim().into(),
        }));
    }

    lines.sort_by_key(|line| line.start_ms);
    let first = lines.first()?.start_ms;
    if first > 300 {
        lines.insert(
            0,
            LyricLine {
                start_ms: 0,
                end_ms: None,
                text: String::new(),
            },
        );
    }
    for index in 0..lines.len() {
        lines[index].end_ms = lines
            .get(index + 1)
            .map(|line| line.start_ms)
            .or_else(|| lines[index].start_ms.checked_add(5000));
    }
    Some(lines)
}

fn lrc_timestamp(stamp: &Captures<'_>) -> Option<i64> {
    let minutes: i64 = stamp[1].parse().ok()?;
    let seconds: i64 = stamp[2].parse().ok()?;
    let fraction: i64 = stamp[3].parse().ok()?;
    let millis = if stamp[3].len() == 2 {
        fraction * 10
    } else {
        fraction
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds * 1000 + millis)
}
