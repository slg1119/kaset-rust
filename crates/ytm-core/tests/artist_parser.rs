use serde_json::{Value, json};
use ytm_core::models::ShelfItem;
use ytm_core::parsers::artist::{parse_artist, parse_artist_items};

fn response(sections: Value) -> Value {
    json!({"contents":{"singleColumnBrowseResultsRenderer":{"tabs":[
        {"tabRenderer":{"content":{"sectionListRenderer":{"contents":sections}}}}
    ]}}})
}

#[test]
fn reads_artist_header_and_ordered_sections_with_more_endpoints() {
    let page =
        parse_artist(&serde_json::from_str(include_str!("fixtures/artist_response.json")).unwrap())
            .unwrap();
    assert_eq!(page.name, "Test Artist");
    assert_eq!(page.description.as_deref(), Some("Artist biography"));
    assert_eq!(
        page.thumbnail_url.as_deref(),
        Some("https://lh3.googleusercontent.com/test-artist")
    );
    assert_eq!(page.subscribers.as_deref(), Some("123 subscribers"));
    assert_eq!(page.channel_id.as_deref(), Some("UC-test"));
    assert_eq!(page.subscribed, Some(false));
    assert_eq!(page.top_songs_section, Some(1));
    assert_eq!(page.radio_playlist_id.as_deref(), Some("RD-test"));
    assert_eq!(page.shuffle_playlist_id.as_deref(), Some("RDAO-test"));
    assert_eq!(page.shuffle_params.as_deref(), Some("test%3D"));
    assert_eq!(
        page.sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>(),
        ["Videos", "Top songs", "Albums"]
    );
    assert!(
        matches!(&page.sections[1].items[0], ShelfItem::Song(song) if song.artists[0].browse_id.as_deref() == Some("UC-test"))
    );
    assert_eq!(
        page.sections[1].more.as_ref().unwrap().browse_id,
        "test-songs"
    );
    assert_eq!(
        page.sections[2].more.as_ref().unwrap().params.as_deref(),
        Some("test%3D")
    );
}

#[test]
fn reads_grid_items_and_playlist_shelf_items_in_order() {
    let items = parse_artist_items(&response(json!([
        {"gridRenderer":{"items":[
            {"musicTwoRowItemRenderer":{"title":{"runs":[{"text":"Test album"}]},"navigationEndpoint":{"browseEndpoint":{"browseId":"MPRE-test"}}}},
            {"musicMultiRowListItemRenderer":{"title":{"runs":[{"text":"Test episode"}]},"onTap":{"watchEndpoint":{"videoId":"test-episode"}}}}
        ]}},
        {"musicPlaylistShelfRenderer":{"contents":[
            {"musicResponsiveListItemRenderer":{"playlistItemData":{"videoId":"test-song"},
                "flexColumns":[{"musicResponsiveListItemFlexColumnRenderer":{"text":{"runs":[{"text":"Test song"}]}}}]}}
        ]}}
    ]))).unwrap();
    assert!(
        matches!(&items[..], [ShelfItem::Album(_), ShelfItem::Song(episode), ShelfItem::Song(song)] if episode.video_id == "test-episode" && song.video_id == "test-song")
    );
    assert!(parse_artist_items(&json!({})).is_err());
}

#[test]
fn accepts_visual_header_with_missing_optional_fields() {
    let page = parse_artist(&json!({"header":{"musicVisualHeaderRenderer":{"title":{"runs":[{"text":"Test Artist"}]}}}})).unwrap();
    assert_eq!(page.name, "Test Artist");
    assert_eq!(page.subscribed, None);
    assert!(page.sections.is_empty());
    assert!(parse_artist(&json!({})).is_err());
}
