use serde_json::{Value, json};
use ytm_core::models::ShelfItem;
use ytm_core::parsers::home::parse_home;
use ytm_core::parsers::podcasts::{
    parse_podcast_episodes_continuation, parse_podcast_show, parse_podcasts,
    parse_podcasts_continuation,
};

#[test]
fn parses_discovery_and_home_podcast_items() {
    let value = serde_json::from_str(include_str!("fixtures/podcasts.json")).unwrap();
    let feed = parse_podcasts(&value).unwrap();
    assert_eq!(feed.continuation.as_deref(), Some("test-continuation"));
    assert_eq!(feed.sections[0].title, "Recommended shows");
    let items = &feed.sections[0].items;
    assert!(matches!(&items[0], ShelfItem::Podcast(card) if card.browse_id == "MPSPPtest"));
    assert!(matches!(&items[1], ShelfItem::Episode(episode) if
        episode.video_id == "test-video1" && episode.show_browse_id.as_deref() == Some("MPSPPtest")));
    assert_eq!(parse_home(&value).unwrap().sections, feed.sections);
}

#[test]
fn parses_show_metadata_and_episode_shelf() {
    let value = serde_json::from_str(include_str!("fixtures/podcast_show.json")).unwrap();
    let page = parse_podcast_show(&value).unwrap();
    assert_eq!(page.title, "Test show");
    assert_eq!(page.author.as_deref(), Some("Test host"));
    assert_eq!(page.description.as_deref(), Some("About the show"));
    assert_eq!(
        page.thumbnail_url.as_deref(),
        Some("https://i.ytimg.com/test-show")
    );
    assert_eq!(page.continuation.as_deref(), Some("test-continuation"));
    assert_eq!(page.episodes.len(), 1);
    let episode = &page.episodes[0];
    assert_eq!(episode.video_id, "test-video1");
    assert_eq!(episode.title, "Test episode");
    assert_eq!(episode.description.as_deref(), Some("About the episode"));
    assert_eq!(episode.date.as_deref(), Some("Nov 22, 2026"));
    assert_eq!(episode.duration.as_deref(), Some("42 min"));
    assert_eq!(
        episode.thumbnail_url.as_deref(),
        Some("https://i.ytimg.com/test-episode")
    );
    assert_eq!(episode.progress, Some(0.25));
}

#[test]
fn parses_legacy_show_and_episode_continuation() {
    let value: Value = serde_json::from_str(include_str!("fixtures/podcast_show.json")).unwrap();
    let contents = &value["contents"]["twoColumnBrowseResultsRenderer"];
    let header = &contents["tabs"][0]["tabRenderer"]["content"]["sectionListRenderer"]["contents"]
        [0]["musicResponsiveHeaderRenderer"];
    let shelf =
        &contents["secondaryContents"]["sectionListRenderer"]["contents"][0]["musicShelfRenderer"];
    let legacy = json!({
        "header": {"musicDetailHeaderRenderer": header},
        "contents": {"singleColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {"content": contents["secondaryContents"]}}]}}
    });
    assert_eq!(
        parse_podcast_show(&legacy).unwrap(),
        parse_podcast_show(&value).unwrap()
    );

    let mut shelf = shelf.clone();
    shelf["contents"][0]["musicMultiRowListItemRenderer"]["playedText"] =
        json!({"runs": [{"text": "Played"}]});
    let mut response = json!({"continuationContents": {"musicShelfContinuation": shelf}});
    let page = parse_podcast_episodes_continuation(&response).unwrap();
    assert_eq!(page.episodes[0].progress, Some(1.0));
    assert_eq!(page.continuation.as_deref(), Some("test-continuation"));
    response["continuationContents"]["musicShelfContinuation"]["continuations"] = json!([]);
    assert!(
        parse_podcast_episodes_continuation(&response)
            .unwrap()
            .continuation
            .is_none()
    );
    assert!(parse_podcast_episodes_continuation(&json!({})).is_err());
}

#[test]
fn parses_carousel_discovery_continuation() {
    let value: Value = serde_json::from_str(include_str!("fixtures/podcasts.json")).unwrap();
    let list = &value["contents"]["singleColumnBrowseResultsRenderer"]["tabs"][0]["tabRenderer"]["content"]
        ["sectionListRenderer"];
    let mut carousel =
        list["contents"][0]["itemSectionRenderer"]["contents"][0]["musicCarouselShelfRenderer"]
            .clone();
    carousel["continuations"] = list["continuations"].clone();
    let response = json!({"continuationContents": {"musicCarouselShelfContinuation": carousel}});
    let page = parse_podcasts_continuation(&response).unwrap();
    assert_eq!(page.continuation.as_deref(), Some("test-continuation"));
    assert_eq!(page.sections[0].items.len(), 2);
}

#[test]
fn parses_responsive_episode_fallback_and_ignores_show_cards_in_the_episode_list() {
    let shelf = json!({
        "contents": [
            {"musicResponsiveListItemRenderer": {
                "playlistItemData": {"videoId": "test-video2"},
                "flexColumns": [
                    {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Fallback episode"}]}}},
                    {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Test show"}]}}}
                ]
            }},
            {"musicTwoRowItemRenderer": {
                "title": {"runs": [{"text": "Other show"}]},
                "navigationEndpoint": {"browseEndpoint": {"browseId": "MPSPPother"}}
            }}
        ]
    });
    let response = json!({"continuationContents": {"musicShelfContinuation": shelf}});
    let page = parse_podcast_episodes_continuation(&response).unwrap();
    assert_eq!(page.episodes.len(), 1);
    assert_eq!(page.episodes[0].video_id, "test-video2");
    assert_eq!(page.episodes[0].title, "Fallback episode");
    assert_eq!(page.episodes[0].show_title.as_deref(), Some("Test show"));
    assert_eq!(page.episodes[0].progress, None);
}

#[test]
fn task12_show_without_a_title_still_parses_episodes() {
    let mut value: Value =
        serde_json::from_str(include_str!("fixtures/podcast_show.json")).unwrap();
    let header = &mut value["contents"]["twoColumnBrowseResultsRenderer"]["tabs"][0]["tabRenderer"]
        ["content"]["sectionListRenderer"]["contents"][0]["musicResponsiveHeaderRenderer"];
    header.as_object_mut().unwrap().remove("title");
    let page = parse_podcast_show(&value).unwrap();
    assert!(page.title.is_empty());
    assert_eq!(page.episodes[0].title, "Test episode");
    assert_eq!(page.continuation.as_deref(), Some("test-continuation"));
}
