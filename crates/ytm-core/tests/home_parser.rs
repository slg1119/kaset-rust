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
