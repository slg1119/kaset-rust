use serde_json::Value;
use ytm_core::models::LikeStatus;
use ytm_core::parsers::account_actions::{parse_add_to_playlist, parse_song_metadata};

#[test]
fn extracts_rating_and_both_feedback_tokens_from_wrapped_queue_item() {
    let response: Value =
        serde_json::from_str(include_str!("fixtures/song_metadata.json")).unwrap();
    let metadata = parse_song_metadata(&response).unwrap();
    assert_eq!(metadata.like_status, Some(LikeStatus::Dislike));
    assert!(metadata.in_library);
    assert_eq!(metadata.add_token.unwrap().expose(), "test-add");
    assert_eq!(metadata.remove_token.unwrap().expose(), "test-remove");
}

#[test]
fn lists_distinct_playlist_options_and_exposes_create_affordance() {
    let response: Value =
        serde_json::from_str(include_str!("fixtures/add_to_playlist.json")).unwrap();
    let menu = parse_add_to_playlist(&response);
    assert!(menu.can_create_playlist);
    assert_eq!(menu.title.as_deref(), Some("Add to playlist"));
    assert_eq!(menu.options.len(), 1);
    assert_eq!(menu.options[0].playlist_id, "PLtest");
    assert_eq!(menu.options[0].title, "Mix");
    assert!(menu.options[0].is_selected);
}

#[test]
fn playlist_track_ids_keep_duplicate_videos_distinct_through_continuation() {
    use serde_json::json;
    use ytm_core::parsers::browse_page::{
        parse_browse_page_continuation, parse_browse_page_for_id,
    };

    let mut first = json!({"musicResponsiveListItemRenderer": {
        "playlistItemData": {"videoId": "song-id", "playlistSetVideoId": "first-id"},
        "flexColumns": [{"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs":[{"text":"Track"}]}}}]
    }});
    let mut second = first.clone();
    let second_renderer = &mut second["musicResponsiveListItemRenderer"];
    second_renderer["playlistItemData"] = json!({"videoId": "song-id"});
    second_renderer["menu"] = json!({"menuRenderer":{"items":[{"menuServiceItemRenderer":{
        "serviceEndpoint":{"playlistEditEndpoint":{"actions":[{"action":"ACTION_REMOVE_VIDEO","setVideoId":"second-id"}]}}
    }}]}});
    let page = parse_browse_page_for_id(&json!({
        "header":{"musicEditablePlaylistDetailHeaderRenderer":{"header":{
            "musicDetailHeaderRenderer":{"title":{"runs":[{"text":"My playlist"}]}}
        }}},
        "contents":{"sectionListRenderer":{"contents":[{"musicPlaylistShelfRenderer":{"contents":[first, second]}}]}}
    }), "VLPLtest").unwrap();
    assert!(page.playlist.unwrap().can_delete);
    assert_eq!(page.songs[0].video_id, page.songs[1].video_id);
    assert_eq!(
        page.set_video_ids.unwrap(),
        [Some("first-id".into()), Some("second-id".into())]
    );
    first["musicResponsiveListItemRenderer"]["playlistItemData"] = json!({"videoId":"song-id"});
    let page = parse_browse_page_continuation(&json!({
        "continuationContents":{"musicPlaylistShelfContinuation":{"contents":[first, second]}}
    }))
    .unwrap();
    assert_eq!(page.songs.len(), 2);
    assert_eq!(
        page.set_video_ids.unwrap(),
        [None, Some("second-id".into())]
    );
}

#[test]
fn direct_queue_items_use_the_active_rating_toggle_and_ignore_other_songs() {
    use serde_json::json;

    let mut response: Value =
        serde_json::from_str(include_str!("fixtures/song_metadata.json")).unwrap();
    const FIRST: &str = concat!(
        "/contents/singleColumnMusicWatchNextResultsRenderer/tabbedRenderer/",
        "watchNextTabbedResultsRenderer/tabs/0/tabRenderer/content/musicQueueRenderer/",
        "content/playlistPanelRenderer/contents/0",
    );
    let first = response.pointer_mut(FIRST).unwrap();
    let mut renderer =
        first["playlistPanelVideoWrapperRenderer"]["primaryRenderer"]["playlistPanelVideoRenderer"]
            .take();
    let menu = &mut renderer["menu"]["menuRenderer"];
    menu["topLevelButtons"] = json!([]);
    menu["items"].as_array_mut().unwrap().push(json!({"toggleMenuServiceItemRenderer": {
        "isToggled": true,
        "defaultServiceEndpoint": {"likeEndpoint":{"status":"LIKE"}},
        "toggledServiceEndpoint": {"likeEndpoint":{"status":"INDIFFERENT","target":{"videoId":"other-id"}}}
    }}));
    *first = json!({"playlistPanelVideoRenderer": renderer});
    assert_eq!(parse_song_metadata(&response).unwrap().like_status, None);
    let first = response.pointer_mut(FIRST).unwrap();
    first["playlistPanelVideoRenderer"]["menu"]["menuRenderer"]["items"][1]["toggleMenuServiceItemRenderer"]
        ["toggledServiceEndpoint"]["likeEndpoint"]["target"]["videoId"] = json!("song-id");
    assert_eq!(
        parse_song_metadata(&response).unwrap().like_status,
        Some(LikeStatus::Like)
    );
}
