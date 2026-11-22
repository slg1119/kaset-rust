use serde_json::{Value, json};
use ytm_core::models::{Chip, ShelfItem};
use ytm_core::parsers::home::{parse_feed, parse_feed_continuation};

#[test]
fn parses_feed_chips_and_category_grid() {
    let response = serde_json::from_str(include_str!("fixtures/feed_response.json")).unwrap();
    let page = parse_feed(&response).unwrap();

    assert_eq!(
        page.chips,
        [Chip {
            title: "Focus".into(),
            params: "test-chip%3D".into()
        }]
    );
    assert_eq!(page.continuation.as_deref(), Some("test-continuation"));
    assert_eq!(page.sections[0].title, "Genres");
    assert_eq!(
        page.sections[0].items,
        [
            ShelfItem::Category {
                title: "Jazz".into(),
                browse_id: "FEmusic_moods_and_genres_category".into(),
                params: Some("test-jazz".into()),
                color: Some("#112233".into()),
            },
            ShelfItem::Category {
                title: "Blues".into(),
                browse_id: "test-category".into(),
                params: None,
                color: None,
            },
        ]
    );
}

#[test]
fn parses_continuation_sections_and_stops_when_the_next_token_is_absent() {
    let mut response: Value =
        serde_json::from_str(include_str!("fixtures/feed_continuation.json")).unwrap();
    let page = parse_feed_continuation(&response).unwrap();
    assert!(page.chips.is_empty());
    assert_eq!(page.continuation.as_deref(), Some("test-next-continuation"));
    assert_eq!(page.sections[0].title, "More songs");
    assert!(
        matches!(&page.sections[0].items[0], ShelfItem::Song(song) if song.video_id == "test-video1")
    );

    response["continuationContents"]["sectionListContinuation"]["continuations"] = json!([]);
    assert_eq!(
        parse_feed_continuation(&response).unwrap().continuation,
        None
    );
    assert!(parse_feed_continuation(&json!({})).is_err());
}
