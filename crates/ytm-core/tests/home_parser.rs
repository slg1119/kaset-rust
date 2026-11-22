use serde_json::{Value, json};
use ytm_core::YtmError;
use ytm_core::models::{ArtistRef, ShelfItem};
use ytm_core::parsers::home::parse_home;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/home_response.json")).unwrap()
}

#[test]
fn parses_carousel_sections() {
    let page = parse_home(&fixture()).unwrap();

    let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Quick picks", "Recommended albums"]);
    assert_eq!(page.continuation, None);

    let ShelfItem::Song(song) = &page.sections[0].items[0] else {
        panic!("expected song, got {:?}", page.sections[0].items[0]);
    };
    assert_eq!(song.video_id, "dQw4w9WgXcQ");
    assert_eq!(song.title, "Never Gonna Give You Up");
    assert_eq!(
        song.artists,
        [ArtistRef {
            name: "Rick Astley".into(),
            browse_id: Some("UCuAXFkgsw1L7xaCfnd5JJOw".into()),
        }]
    );

    let ShelfItem::Album(album) = &page.sections[1].items[0] else {
        panic!("expected album, got {:?}", page.sections[1].items[0]);
    };
    assert_eq!(album.browse_id, "MPREb_test123");
    assert_eq!(album.subtitle.as_deref(), Some("Test Artist • 2024"));
    assert_eq!(
        album.thumbnail_url.as_deref(),
        Some("https://example.com/album.jpg")
    );

    insta::assert_yaml_snapshot!(page);
}

#[test]
fn missing_section_list_is_parse_error() {
    let err = parse_home(&json!({ "contents": {} })).unwrap_err();
    assert!(matches!(err, YtmError::Parse { .. }), "{err:?}");
}

#[test]
fn unsupported_sections_are_skipped() {
    let response = json!({ "contents": { "singleColumnBrowseResultsRenderer": { "tabs": [ { "tabRenderer": {
        "content": { "sectionListRenderer": { "contents": [ { "somethingNewRenderer": {} } ] } } } } ] } } });
    assert!(parse_home(&response).unwrap().sections.is_empty());
}

fn carousel(items: Vec<Value>) -> Vec<ShelfItem> {
    let response = json!({
        "contents": { "singleColumnBrowseResultsRenderer": { "tabs": [{
            "tabRenderer": { "content": { "sectionListRenderer": { "contents": [{
                "musicCarouselShelfRenderer": { "contents": items }
            }] } } }
        }] } }
    });
    parse_home(&response)
        .unwrap()
        .sections
        .into_iter()
        .flat_map(|section| section.items)
        .collect()
}

fn two_row_song(subtitle: Value) -> ShelfItem {
    carousel(vec![json!({
        "musicTwoRowItemRenderer": {
            "title": { "runs": [{ "text": "Song title" }] },
            "navigationEndpoint": { "watchEndpoint": { "videoId": "video" } },
            "subtitle": { "runs": subtitle }
        }
    })])
    .remove(0)
}

#[test]
fn two_row_keeps_unlinked_co_artists_in_linked_fields() {
    let ShelfItem::Song(song) = two_row_song(json!([
        { "text": " A ", "navigationEndpoint": { "browseEndpoint": { "browseId": "UC_artist" } } },
        { "text": " & " },
        { "text": " B " },
        { "text": " • " },
        { "text": "2024" },
        { "text": "Album title" }
    ])) else {
        panic!("expected a song");
    };

    assert_eq!(
        song.artists,
        [
            ArtistRef {
                name: "A".into(),
                browse_id: Some("UC_artist".into())
            },
            ArtistRef {
                name: "B".into(),
                browse_id: None
            }
        ]
    );
}

#[test]
fn two_row_falls_back_to_first_artist_like_field() {
    let ShelfItem::Song(song) = two_row_song(json!([
        { "text": " • " },
        { "text": "싱글" },
        { "text": " 1.2K views " },
        { "text": " 1,234 tracks " },
        { "text": " 4:55 " },
        { "text": " 4 minutes, 55 seconds " },
        { "text": "2024" },
        { "text": " · " },
        { "text": "," },
        { "text": " Plain artist " },
        { "text": " & " },
        { "text": "Co-artist" },
        { "text": "•" },
        { "text": "Album title" }
    ])) else {
        panic!("expected a song");
    };

    assert_eq!(
        song.artists,
        [
            ArtistRef {
                name: "Plain artist".into(),
                browse_id: None
            },
            ArtistRef {
                name: "Co-artist".into(),
                browse_id: None
            }
        ]
    );
}

#[test]
fn two_row_accepts_artist_page_type_for_linked_run() {
    let ShelfItem::Song(song) = two_row_song(json!([
        { "text": "Album title" },
        { "text": "•" },
        { "text": "2024", "navigationEndpoint": { "browseEndpoint": {
            "browseId": "artist_destination",
            "browseEndpointContextSupportedConfigs": {
                "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ARTIST" }
            }
        } } }
    ])) else {
        panic!("expected a song");
    };

    assert_eq!(
        song.artists,
        [ArtistRef {
            name: "2024".into(),
            browse_id: Some("artist_destination".into())
        }]
    );
}

#[test]
fn classifies_library_artists_by_page_type_and_prefix() {
    let mut browse = json!({
        "browseId": "MPLAUC_library_artist",
        "browseEndpointContextSupportedConfigs": {
            "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_LIBRARY_ARTIST" }
        }
    });
    let card = |browse| {
        json!({
            "musicTwoRowItemRenderer": {
                "title": { "runs": [{ "text": "Library artist" }] },
                "navigationEndpoint": { "browseEndpoint": browse }
            }
        })
    };
    let by_page_type_and_prefix = card(browse.clone());
    browse["browseId"] = json!("artist_destination");
    let by_page_type = card(browse.clone());
    browse = json!({ "browseId": "MPLAUC_library_artist" });
    let by_prefix = card(browse);

    let items = carousel(vec![by_page_type_and_prefix, by_page_type, by_prefix]);
    assert_eq!(items.len(), 3);
    for (item, id) in items.iter().zip([
        "MPLAUC_library_artist",
        "artist_destination",
        "MPLAUC_library_artist",
    ]) {
        let ShelfItem::Artist(artist) = item else {
            panic!("expected an artist, got {item:?}");
        };
        assert_eq!(artist.browse_id, id);
    }
}

#[test]
fn responsive_video_id_uses_swift_precedence() {
    let mut renderer = json!({
        "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": {
            "text": { "runs": [{ "text": "Song title" }] }
        } }],
        "navigationEndpoint": { "watchEndpoint": { "videoId": "watch_id" } }
    });
    let watch_only = renderer.clone();
    renderer["overlay"] = json!({
        "musicItemThumbnailOverlayRenderer": { "content": { "musicPlayButtonRenderer": {
            "playNavigationEndpoint": { "watchEndpoint": { "videoId": "overlay_id" } }
        } } }
    });
    let watch_and_overlay = renderer.clone();
    renderer["playlistItemData"] = json!({ "videoId": "playlist_id" });
    let all_sources = renderer.clone();
    renderer.as_object_mut().unwrap().remove("playlistItemData");
    renderer
        .as_object_mut()
        .unwrap()
        .remove("navigationEndpoint");
    let overlay_only = renderer;

    let items = carousel(
        [watch_only, watch_and_overlay, all_sources, overlay_only]
            .into_iter()
            .map(|renderer| json!({ "musicResponsiveListItemRenderer": renderer }))
            .collect(),
    );
    assert_eq!(items.len(), 4);
    for (item, id) in items
        .iter()
        .zip(["watch_id", "watch_id", "playlist_id", "overlay_id"])
    {
        let ShelfItem::Song(song) = item else {
            panic!("expected a song, got {item:?}");
        };
        assert_eq!(song.video_id, id);
    }
}

#[test]
fn responsive_artists_prefer_navigable_links_over_plain_text_fallback() {
    let row = |runs| {
        json!({
            "musicResponsiveListItemRenderer": {
                "playlistItemData": { "videoId": "video" },
                "flexColumns": [
                    { "musicResponsiveListItemFlexColumnRenderer": {
                        "text": { "runs": [{ "text": "Song title" }] }
                    } },
                    { "musicResponsiveListItemFlexColumnRenderer": { "text": { "runs": runs } } }
                ]
            }
        })
    };
    let items = carousel(vec![
        row(json!([
            { "text": "Plain artist" },
            { "text": " & " },
            { "text": " 311 ", "navigationEndpoint": { "browseEndpoint": {
                "browseId": "MPLAUC_library_artist"
            } } }
        ])),
        row(json!([
            { "text": " • " },
            { "text": "2024" },
            { "text": "Invalid link", "navigationEndpoint": { "browseEndpoint": {
                "browseId": "artist_destination",
                "browseEndpointContextSupportedConfigs": {
                    "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_ARTIST" }
                }
            } } },
            { "text": " Uploaded artist " },
            { "text": " & " },
            { "text": "Second plain artist" }
        ])),
    ]);
    assert_eq!(items.len(), 2);
    for (item, artist) in items.iter().zip([
        ArtistRef {
            name: "311".into(),
            browse_id: Some("MPLAUC_library_artist".into()),
        },
        ArtistRef {
            name: "Uploaded artist".into(),
            browse_id: None,
        },
    ]) {
        let ShelfItem::Song(song) = item else {
            panic!("expected a song, got {item:?}");
        };
        assert_eq!(song.artists, [artist]);
    }
}

#[test]
fn parses_every_supported_shelf_type() {
    let response: Value =
        serde_json::from_str(include_str!("fixtures/home_shelf_types.json")).unwrap();
    let page = parse_home(&response).unwrap();

    let titles: Vec<&str> = page.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Top songs",
            "Mixed for you",
            "Featured",
            "Top artists",
            "Wrapped carousel"
        ]
    );
    assert_eq!(page.continuation.as_deref(), Some("test-home-continuation"));
    assert!(
        matches!(&page.sections[0].items[0], ShelfItem::Song(song) if song.video_id == "test-vid-01")
    );
    assert!(
        matches!(&page.sections[1].items[0], ShelfItem::Playlist(card) if card.browse_id == "VLtest-playlist")
    );
    assert!(
        matches!(&page.sections[2].items[0], ShelfItem::Album(card) if card.browse_id == "MPREtest-album")
    );
    assert!(
        matches!(&page.sections[3].items[0], ShelfItem::Artist(card) if card.browse_id == "UCgrid-artist")
    );
    assert!(
        matches!(&page.sections[4].items[0], ShelfItem::Song(song) if song.video_id == "test-vid-03")
    );

    insta::assert_yaml_snapshot!(page);
}

#[test]
fn item_section_without_a_supported_shelf_is_skipped() {
    let response = json!({ "contents": { "singleColumnBrowseResultsRenderer": { "tabs": [ { "tabRenderer": {
        "content": { "sectionListRenderer": { "contents": [
            { "itemSectionRenderer": { "contents": [ { "messageRenderer": {} } ] } }
        ] } } } } ] } } });
    assert!(parse_home(&response).unwrap().sections.is_empty());
}
