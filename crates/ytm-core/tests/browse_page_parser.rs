use serde_json::{Value, json};
use ytm_core::YtmError;
use ytm_core::client::playlist_browse_id;
use ytm_core::parsers::browse_page::{parse_browse_page, parse_browse_page_for_id};

const ART: &str = "https://lh3.googleusercontent.com/test-cover";

fn track(id: &str, title: &str, artist: Option<&str>) -> Value {
    let mut columns = vec![json!({
        "musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": title}]}}
    })];
    if let Some(artist) = artist {
        columns.push(json!({
            "musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": artist}]}}
        }));
    }
    json!({"musicResponsiveListItemRenderer": {
        "playlistItemData": {"videoId": id}, "flexColumns": columns
    }})
}

fn two_column(header: Value, sections: Value) -> Value {
    json!({"contents": {"twoColumnBrowseResultsRenderer": {
        "tabs": [{"tabRenderer": {"content": {"sectionListRenderer": {"contents": [
            {"musicResponsiveHeaderRenderer": header}
        ]}}}}],
        "secondaryContents": {"sectionListRenderer": {"contents": sections}}
    }}})
}

#[test]
fn album_reads_header_and_tracks_with_cover_fallback() {
    let page = parse_browse_page_for_id(&two_column(
        json!({
            "title": {"runs": [{"text": "Test Album"}]},
            "subtitle": {"runs": [{"text": "Album"}, {"text": " • "}, {"text": "Test Artist"}]},
            "secondSubtitle": {"runs": [{"text": "2 songs"}]},
            "thumbnail": {"musicThumbnailRenderer": {"thumbnail": {"thumbnails": [{"url": ART}]}}}
        }),
        json!([{"musicShelfRenderer": {"contents": [
            track("test-video1", "Track One", Some("Test Artist")),
            track("test-video2", "Track Two", None),
            track("", "Unavailable", None)
        ]}}]),
    ), "MPREb_test_album")
    .unwrap();
    assert_eq!(page.title, "Test Album");
    assert_eq!(page.subtitle.as_deref(), Some("Album • Test Artist"));
    assert_eq!(page.second_subtitle.as_deref(), Some("2 songs"));
    assert_eq!(page.thumbnail_url.as_deref(), Some(ART));
    assert_eq!(page.songs.len(), 2);
    assert!(page.songs.iter().all(|song| {
        song.album.as_ref().is_some_and(|album| {
            album.browse_id == "MPREb_test_album" && album.title == "Test Album"
        })
    }));
    assert_eq!(page.songs[0].artists[0].name, "Test Artist");
    assert_eq!(page.songs[1].thumbnail_url.as_deref(), Some(ART));
    assert!(page.songs[1].artists.is_empty());
    assert_eq!(page.continuation, None);
}

#[test]
fn playlist_uses_its_track_shelf_and_trailing_continuation() {
    let page = parse_browse_page(&two_column(
        json!({
            "title": {"runs": [{"text": "Test Playlist"}]},
            "straplineTextOne": {"runs": [{"text": "Test Curator"}]},
            "subtitle": {"runs": [{"text": "Playlist • Public"}]}
        }),
        json!([
            {"musicShelfRenderer": {"contents": [track("test-video1", "Suggestion", None)]}},
            {"musicPlaylistShelfRenderer": {"contents": [
                track("test-video2", "Playlist Track", None),
                {"continuationItemRenderer": {
                    "continuationEndpoint": {"continuationCommand": {"token": "test-next"}}
                }}
            ]}}
        ]),
    ))
    .unwrap();
    assert_eq!(page.title, "Test Playlist");
    assert_eq!(page.subtitle.as_deref(), Some("Test Curator"));
    assert_eq!(page.songs.len(), 1);
    assert_eq!(page.songs[0].video_id, "test-video2");
    assert_eq!(page.continuation.as_deref(), Some("test-next"));
}

#[test]
fn legacy_header_reads_single_column_shelf_and_continuation() {
    let page = parse_browse_page(&json!({
        "header": {"musicDetailHeaderRenderer": {"title": {"runs": [{"text": "Legacy Playlist"}]}}},
        "contents": {"singleColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"content": {
            "sectionListRenderer": {"contents": [{"musicPlaylistShelfRenderer": {
                "contents": [track("test-video1", "Legacy Track", None)],
                "continuations": [{"nextContinuationData": {"continuation": "test-legacy-next"}}]
            }}]}
        }}}]}}
    }))
    .unwrap();
    assert_eq!(page.title, "Legacy Playlist");
    assert_eq!(page.songs[0].title, "Legacy Track");
    assert_eq!(page.continuation.as_deref(), Some("test-legacy-next"));
}

#[test]
fn missing_header_is_a_parse_error() {
    assert!(matches!(
        parse_browse_page(&json!({})),
        Err(YtmError::Parse { .. })
    ));
}

#[test]
fn normalizes_playlist_ids_like_swift() {
    for (id, expected) in [
        ("PLtest", "VLPLtest"),
        ("OLAKtest", "VLOLAKtest"),
        ("LL", "VLLL"),
        ("VMtest", "VLVMtest"),
        ("RDtest", "RDtest"),
        ("VLPLtest", "VLPLtest"),
        ("MPREb_test", "MPREb_test"),
        ("UCtest", "UCtest"),
        (
            "FEmusic_library_privately_owned_tracks",
            "FEmusic_library_privately_owned_tracks",
        ),
    ] {
        assert_eq!(playlist_browse_id(id), expected);
    }
}

#[test]
fn playlist_without_a_track_shelf_ignores_suggestions() {
    let page = parse_browse_page(&two_column(
        json!({"title": {"runs": [{"text": "Empty Playlist"}]}}),
        json!([{"musicShelfRenderer": {"contents": [
            track("test-video1", "Suggestion", None)
        ]}}]),
    ))
    .unwrap();
    assert!(page.songs.is_empty());
}

#[test]
fn known_playlist_id_wins_over_an_album_like_subtitle() {
    let value = two_column(
        json!({
            "title": {"runs": [{"text": "Test Playlist"}]},
            "subtitle": {"runs": [{"text": "Album"}]}
        }),
        json!([{"musicShelfRenderer": {"contents": [
            track("test-video1", "Suggestion", None)
        ]}}]),
    );
    assert!(
        parse_browse_page_for_id(&value, "VLPLtest")
            .unwrap()
            .songs
            .is_empty()
    );
}

#[test]
fn album_playlist_without_a_header_uses_the_route_title() {
    let value = json!({"contents": {"sectionListRenderer": {"contents": [
        {"musicPlaylistShelfRenderer": {"contents": [track("test-video1", "Album track", None)]}}
    ]}}});
    let page = parse_browse_page_for_id(&value, "VLOLAKtest").unwrap();
    assert!(page.title.is_empty());
    assert_eq!(page.songs[0].title, "Album track");
    assert!(parse_browse_page_for_id(&value, "VLPLtest").is_err());
}

#[test]
fn task12_album_fallback_excludes_suggested_shelves() {
    let response = two_column(
        json!({"title": {"runs": [{"text": "Test album"}]}}),
        json!([
            {"musicShelfRenderer": {"title": {"simpleText": " Suggestions "}, "contents": [track("test-video1", "Suggested", None)]}},
            {"musicShelfRenderer": {"header": {"content": "More Suggestions"}, "contents": [track("test-video2", "Suggested", None)]}},
            {"musicShelfRenderer": {"strapline": [{"text": {"runs": [{"text": "Suggested"}]}}], "contents": [track("test-video3", "Suggested", None)]}},
            {"musicShelfRenderer": {"title": {"simpleText": "Album Tracks"}, "contents": [track("test-video4", "Album track", None)]}}
        ]),
    );
    let page = parse_browse_page_for_id(&response, "MPREtest").unwrap();
    assert_eq!(
        page.songs
            .into_iter()
            .map(|song| song.title)
            .collect::<Vec<_>>(),
        ["Album track"]
    );
}
