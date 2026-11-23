use serde_json::Value;

use super::helpers::album_from_run;
use crate::YtmError;
use crate::json::{at, str_at, thumbnail_url};
use crate::models::{ArtistRef, RadioQueue, Song};

const RADIO_PANEL: &[&str] = &[
    "contents",
    "singleColumnMusicWatchNextResultsRenderer",
    "tabbedRenderer",
    "watchNextTabbedResultsRenderer",
    "tabs",
    "0",
    "tabRenderer",
    "content",
    "musicQueueRenderer",
    "content",
    "playlistPanelRenderer",
];
const CONTINUATION_PANEL: &[&str] = &["continuationContents", "playlistPanelContinuation"];

pub fn parse_radio(value: &Value) -> Result<RadioQueue, YtmError> {
    parse_panel(value, RADIO_PANEL)
}

pub fn parse_radio_continuation(value: &Value) -> Result<RadioQueue, YtmError> {
    parse_panel(value, CONTINUATION_PANEL)
}

fn parse_panel(value: &Value, path: &[&str]) -> Result<RadioQueue, YtmError> {
    let panel = at(value, path)
        .filter(|panel| panel.is_object())
        .ok_or_else(|| YtmError::Parse {
            endpoint: "next".into(),
            path: path.join("."),
        })?;
    let contents = panel
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(|| YtmError::Parse {
            endpoint: "next".into(),
            path: format!("{}.contents", path.join(".")),
        })?;
    let continuation = str_at(
        panel,
        &[
            "continuations",
            "0",
            "nextRadioContinuationData",
            "continuation",
        ],
    )
    .map(str::to_owned);
    Ok(RadioQueue {
        songs: contents.iter().filter_map(parse_song).collect(),
        continuation,
    })
}

fn parse_song(item: &Value) -> Option<Song> {
    let renderer = item
        .get("playlistPanelVideoRenderer")
        .filter(|renderer| renderer.is_object())
        .or_else(|| {
            at(
                item,
                &[
                    "playlistPanelVideoWrapperRenderer",
                    "primaryRenderer",
                    "playlistPanelVideoRenderer",
                ],
            )
        })?;
    Some(Song {
        album: at(renderer, &["longBylineText", "runs"])
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(album_from_run),
        is_podcast_episode: None,
        video_id: str_at(renderer, &["videoId"])?.into(),
        title: str_at(renderer, &["title", "runs", "0", "text"])?.into(),
        artists: parse_artists(renderer),
        thumbnail_url: thumbnail_url(renderer),
    })
}

fn parse_artists(renderer: &Value) -> Vec<ArtistRef> {
    at(renderer, &["longBylineText", "runs"])
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|run| Some((str_at(run, &["text"])?, run)))
        .take_while(|(text, _)| !matches!(*text, " • " | " · "))
        .filter(|(text, _)| !matches!(*text, " & " | ", "))
        .map(|(name, run)| ArtistRef {
            name: name.into(),
            browse_id: str_at(run, &["navigationEndpoint", "browseEndpoint", "browseId"])
                .map(str::to_owned),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{parse_radio, parse_radio_continuation};
    use crate::YtmError;
    use crate::models::{AlbumRef, ArtistRef, Song};

    #[test]
    fn parses_radio_songs_in_order_and_skips_incomplete_items() {
        let response: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/radio_response.json")).unwrap();
        let queue = parse_radio(&response).unwrap();
        assert_eq!(queue.continuation.as_deref(), Some("test-continuation"));
        assert_eq!(
            queue.songs,
            [
                Song {
                    album: Some(AlbumRef {
                        browse_id: "MPREb_test_album".into(),
                        title: "Test album".into(),
                    }),
                    is_podcast_episode: None,
                    video_id: "test-vid001".into(),
                    title: "Seed song".into(),
                    artists: vec![
                        ArtistRef {
                            name: "First artist".into(),
                            browse_id: Some("test-artist".into()),
                        },
                        ArtistRef {
                            name: "Second artist".into(),
                            browse_id: None,
                        },
                        ArtistRef {
                            name: "Third artist".into(),
                            browse_id: None,
                        },
                    ],
                    thumbnail_url: Some("https://example.com/test-large.jpg".into()),
                },
                Song {
                    album: None,
                    is_podcast_episode: None,
                    video_id: "test-vid002".into(),
                    title: "Wrapped song".into(),
                    artists: vec![],
                    thumbnail_url: None,
                },
            ]
        );
    }

    #[test]
    fn parses_radio_continuation_songs_and_next_token() {
        let response: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/radio_continuation.json"))
                .unwrap();
        let queue = parse_radio_continuation(&response).unwrap();
        assert_eq!(queue.continuation.as_deref(), Some("test-continuation"));
        assert_eq!(
            queue.songs,
            [Song {
                album: None,
                is_podcast_episode: None,
                video_id: "test-vid004".into(),
                title: "More music".into(),
                artists: vec![ArtistRef {
                    name: "Another artist".into(),
                    browse_id: None,
                }],
                thumbnail_url: None,
            }]
        );
    }

    #[test]
    fn accepts_empty_radio_continuation_without_next_token() {
        let response = json!({
            "continuationContents": {"playlistPanelContinuation": {"contents": []}}
        });
        let queue = parse_radio_continuation(&response).unwrap();
        assert!(queue.songs.is_empty());
        assert_eq!(queue.continuation, None);
    }

    #[test]
    fn reports_missing_radio_panel() {
        assert!(matches!(
            parse_radio(&json!({})),
            Err(YtmError::Parse { endpoint, path })
                if endpoint == "next" && path == concat!(
                    "contents.singleColumnMusicWatchNextResultsRenderer.tabbedRenderer.",
                    "watchNextTabbedResultsRenderer.tabs.0.tabRenderer.content.",
                    "musicQueueRenderer.content.playlistPanelRenderer"
                )
        ));
    }

    #[test]
    fn reports_invalid_radio_continuation_contents() {
        let response = json!({
            "continuationContents": {"playlistPanelContinuation": {"contents": {}}}
        });
        assert!(matches!(
            parse_radio_continuation(&response),
            Err(YtmError::Parse { endpoint, path })
                if endpoint == "next" && path == "continuationContents.playlistPanelContinuation.contents"
        ));
    }
}
