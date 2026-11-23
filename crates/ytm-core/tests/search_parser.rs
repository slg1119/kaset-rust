use serde_json::{Value, json};
use ytm_core::models::{ArtistRef, SearchItem};
use ytm_core::parsers::search::{parse_search, parse_search_continuation};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// `kind:id` for each item, like the Swift tests' `allItems.map(\.id)`.
fn ids(items: &[SearchItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            SearchItem::Song(song) => format!("song:{}", song.video_id),
            SearchItem::Video(song) => format!("video:{}", song.video_id),
            SearchItem::Episode(song) => format!("episode:{}", song.video_id),
            SearchItem::Album(card) => format!("album:{}", card.browse_id),
            SearchItem::Playlist(card) => format!("playlist:{}", card.browse_id),
            SearchItem::Artist(card) => format!("artist:{}", card.browse_id),
            SearchItem::Profile(card) => format!("profile:{}", card.browse_id),
            SearchItem::Podcast(card) => format!("podcast:{}", card.browse_id),
        })
        .collect()
}

#[test]
fn parses_shelves_in_server_order() {
    let results = parse_search(&fixture("search_response"));
    assert_eq!(
        ids(&results.items),
        [
            "song:search-video-1",
            "song:search-video-2",
            "album:MPREb_search_album_1",
            "artist:UC-search-result-artist",
        ]
    );
    let SearchItem::Song(song) = &results.items[0] else {
        panic!("expected a song");
    };
    assert_eq!(song.title, "Search Result Song 1");
    let album = song.album.as_ref().expect("linked third flex column");
    assert_eq!(album.browse_id, "MPREb_search_album_1");
    assert_eq!(album.title, "Search Album 1");
    assert_eq!(
        song.artists,
        [ArtistRef {
            name: "Search Artist 1".into(),
            browse_id: Some("UC-search-artist-1".into()),
        }]
    );
    assert_eq!(
        song.thumbnail_url.as_deref(),
        Some("https://example.com/search-thumb-1.jpg")
    );
    assert_eq!(results.continuation, None);
    insta::assert_yaml_snapshot!(results);
}

#[test]
fn direct_item_section_roots_keep_server_order() {
    let results = parse_search(&fixture("search_direct_item_section"));
    assert_eq!(
        ids(&results.items),
        ["song:direct-song-video", "album:MPREdirect-album"]
    );
}

#[test]
fn tabbed_mixed_results_keep_order_and_first_occurrence() {
    let results = parse_search(&fixture("search_tabbed_mixed"));
    assert_eq!(
        ids(&results.items),
        [
            "profile:UCtop-profile",
            "video:official-source-video",
            "podcast:MPSPPfixture-show",
            "episode:fixture-episode-video",
            "playlist:VLfixture-playlist",
        ]
    );
    let SearchItem::Profile(profile) = &results.items[0] else {
        panic!("expected a profile");
    };
    assert_eq!(profile.subtitle.as_deref(), Some("42K subscribers"));
    let SearchItem::Video(video) = &results.items[1] else {
        panic!("expected a video");
    };
    assert_eq!(video.title, "Official Source Video");
    let SearchItem::Episode(episode) = &results.items[3] else {
        panic!("expected an episode");
    };
    assert_eq!(
        episode.artists,
        [ArtistRef {
            name: "Fixture Podcast".into(),
            browse_id: Some("MPSPPfixture-show".into()),
        }]
    );
    assert_eq!(
        results.continuation.as_deref(),
        Some("sanitized-next-page-token")
    );
}

#[test]
fn top_result_watch_destinations_parse_from_every_endpoint_source() {
    let results = parse_search(&fixture("search_top_result_watch_endpoints"));
    assert_eq!(
        ids(&results.items),
        [
            "video:title-endpoint-video",
            "video:on-tap-video",
            "video:thumbnail-overlay-video",
        ]
    );
}

#[test]
fn playlist_ids_in_watch_endpoints_stay_playlists() {
    let results = parse_search(&fixture("search_playlist_watch_endpoints"));
    let playlists: Vec<_> = ids(&results.items)
        .into_iter()
        .filter(|id| !id.starts_with("album:"))
        .collect();
    assert_eq!(
        playlists,
        ["playlist:RDradio-watch", "playlist:VMplaylist-action"]
    );
    assert!(ids(&results.items).contains(&"album:MPREcanonical-album".to_owned()));
}

#[test]
fn later_flex_columns_contribute_metadata() {
    let results = parse_search(&fixture("search_later_flex_columns"));
    let Some(SearchItem::Album(album)) = results.items.first() else {
        panic!("expected an album, got {:?}", results.items);
    };
    assert!(
        album
            .subtitle
            .as_deref()
            .unwrap_or_default()
            .contains("2026"),
        "{album:?}"
    );
}

#[test]
fn missing_section_list_is_an_empty_result() {
    let results = parse_search(&json!({ "contents": {} }));
    assert!(results.items.is_empty());
    assert_eq!(results.continuation, None);
}

fn shelf(items: Value) -> Value {
    json!({ "contents": { "sectionListRenderer": { "contents": [
        { "musicShelfRenderer": { "contents": items } }
    ] } } })
}

#[test]
fn untyped_video_cards_use_the_plain_creator_name() {
    let results = parse_search(&shelf(json!([{ "musicMultiRowListItemRenderer": {
        "title": { "runs": [{ "text": "Creator video" }] },
        "subtitle": { "runs": [{ "text": "Video • Fixture Creator • 1.2M views" }] },
        "onTap": { "watchEndpoint": { "videoId": "creator-video" } }
    } }])));
    let [SearchItem::Video(video)] = results.items.as_slice() else {
        panic!("expected one video, got {:?}", results.items);
    };
    assert_eq!(
        video.artists,
        [ArtistRef {
            name: "Fixture Creator".into(),
            browse_id: None,
        }]
    );
}

#[test]
fn count_only_subtitles_do_not_become_artists() {
    let results = parse_search(&shelf(json!([{ "musicResponsiveListItemRenderer": {
        "playlistItemData": { "videoId": "count-song" },
        "flexColumns": [
            { "musicResponsiveListItemFlexColumnRenderer": {
                "text": { "runs": [{ "text": "Counted" }] } } },
            { "musicResponsiveListItemFlexColumnRenderer": {
                "text": { "runs": [{ "text": "Song" }, { "text": " • " }, { "text": "12 songs" }] } } }
        ]
    } }])));
    let [SearchItem::Song(song)] = results.items.as_slice() else {
        panic!("expected one song, got {:?}", results.items);
    };
    assert!(song.artists.is_empty(), "{song:?}");
}

#[test]
fn library_artist_page_type_is_an_artist() {
    let results = parse_search(&shelf(json!([{ "musicResponsiveListItemRenderer": {
        "navigationEndpoint": { "browseEndpoint": {
            "browseId": "MPLAUC1234567890",
            "browseEndpointContextSupportedConfigs": {
                "browseEndpointContextMusicConfig": { "pageType": "MUSIC_PAGE_TYPE_LIBRARY_ARTIST" }
            }
        } },
        "flexColumns": [{ "musicResponsiveListItemFlexColumnRenderer": {
            "text": { "runs": [{ "text": "Library Artist" }] } } }]
    } }])));
    assert_eq!(ids(&results.items), ["artist:MPLAUC1234567890"]);
}

#[test]
fn shelf_continuation_items_carry_the_next_token() {
    let results = parse_search(&shelf(json!([
        { "continuationItemRenderer": { "continuationEndpoint": {
            "continuationCommand": { "token": "sanitized-item-token" } } } }
    ])));
    assert_eq!(
        results.continuation.as_deref(),
        Some("sanitized-item-token")
    );
}

#[test]
fn filtered_single_shelf_keeps_results_and_continuation() {
    let response = fixture("search_filtered_shelf");
    let results = parse_search(&response);
    assert_eq!(ids(&results.items), ["song:test-song-1"]);
    assert_eq!(results.continuation.as_deref(), Some("test-continuation"));

    let shelf = &response["contents"]["tabbedSearchResultsRenderer"]["tabs"][0]["tabRenderer"]["content"]
        ["sectionListRenderer"]["contents"][0];
    let direct = parse_search(&json!({ "contents": shelf }));
    assert_eq!(results, direct);
}

#[test]
fn search_continuation_combines_shelf_and_actions_in_order() {
    let results = parse_search_continuation(&fixture("search_continuation"));
    assert_eq!(
        ids(&results.items),
        ["song:test-song-2", "video:test-video-3"]
    );
    assert_eq!(results.continuation.as_deref(), Some("test-next-token"));
    let SearchItem::Song(song) = &results.items[0] else {
        panic!("expected a song");
    };
    assert_eq!(song.title, "Second song");
}

#[test]
fn search_continuation_accepts_reload_and_endpoint_envelopes() {
    let fixture = fixture("search_continuation");
    let items = &fixture["onResponseReceivedActions"][0]["appendContinuationItemsAction"]["continuationItems"];
    for envelope in ["onResponseReceivedCommands", "onResponseReceivedEndpoints"] {
        let results = parse_search_continuation(&json!({
            envelope: [{ "reloadContinuationItemsCommand": { "continuationItems": items } }]
        }));
        assert_eq!(
            ids(&results.items),
            ["song:test-song-2", "video:test-video-3"]
        );
        assert_eq!(results.continuation.as_deref(), Some("test-next-token"));
    }
    assert_eq!(parse_search_continuation(&json!({})).continuation, None);
}
