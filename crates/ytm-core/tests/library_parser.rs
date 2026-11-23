use serde_json::{Value, json};
use ytm_core::models::ShelfItem;
use ytm_core::parsers::browse_page::parse_browse_page_continuation;
use ytm_core::parsers::library::{parse_library, parse_library_continuation};

#[test]
fn parses_library_grids_and_nested_artist_shelves() {
    let value = serde_json::from_str(include_str!("fixtures/library_landing.json")).unwrap();
    let page = parse_library(&value).unwrap();
    assert_eq!(page.sections.len(), 2);
    let [ShelfItem::Playlist(playlist), ShelfItem::Album(album)] =
        page.sections[0].items.as_slice()
    else {
        panic!("expected a playlist and album");
    };
    assert_eq!(playlist.subtitle.as_deref(), Some("Test owner"));
    assert_eq!(album.browse_id, "MPREtest");
    assert!(
        matches!(&page.sections[1].items[0], ShelfItem::Artist(artist) if artist.browse_id == "UCtest")
    );
    assert_eq!(page.continuations, ["test-library-next"]);
}

#[test]
fn parses_an_empty_library_grid_continuation() {
    let page = parse_library_continuation(&json!({
        "continuationContents": { "gridContinuation": { "items": [] } }
    }))
    .unwrap();
    assert!(page.sections.is_empty());
    assert!(page.continuations.is_empty());
}

#[test]
fn parses_legacy_and_append_action_continuations_in_order() {
    let legacy: Value =
        serde_json::from_str(include_str!("fixtures/playlist_continuation.json")).unwrap();
    let first = parse_browse_page_continuation(&legacy).unwrap();
    assert_eq!(first.songs[0].title, "Next song");
    assert_eq!(first.continuation.as_deref(), Some("test-next"));

    let row = &legacy["continuationContents"]["musicPlaylistShelfContinuation"]["contents"][0];
    let next = parse_browse_page_continuation(&json!({
        "onResponseReceivedActions": [
            { "ignoredAction": {} },
            { "appendContinuationItemsAction": { "continuationItems": [row, row, {
                "continuationItemRenderer": { "continuationEndpoint": {
                    "continuationCommand": { "token": "test-append-next" }
                } }
            }] } }
        ]
    }))
    .unwrap();
    assert_eq!(next.songs, [first.songs[0].clone(), first.songs[0].clone()]);
    assert_eq!(next.continuation.as_deref(), Some("test-append-next"));
    assert!(parse_browse_page_continuation(&json!({})).is_err());
}

#[test]
fn section_wrapped_playlist_continuation_excludes_recommendations() {
    let legacy: Value =
        serde_json::from_str(include_str!("fixtures/playlist_continuation.json")).unwrap();
    let shelf = &legacy["continuationContents"]["musicPlaylistShelfContinuation"];
    let response = json!({ "continuationContents": { "sectionListContinuation": {
        "contents": [
            { "musicPlaylistShelfRenderer": shelf },
            { "musicShelfRenderer": shelf }
        ]
    } } });
    let page = parse_browse_page_continuation(&response).unwrap();
    assert_eq!(page.songs.len(), 1);
    assert_eq!(page.continuation.as_deref(), Some("test-next"));
}
