use std::net::TcpListener;

use serde_json::json;
use wiremock::matchers::{body_partial_json, header, header_regex, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use ytm_core::YtmError;
use ytm_core::auth::{CookieJar, SharedCookies};
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};

/// Fake value for the client config; never a real credential.
const PLACEHOLDER: &str = "placeholder";

fn config_for(server: &MockServer) -> ClientConfig {
    ClientConfig {
        base_url: server.uri(),
        api_key: PLACEHOLDER.into(),
        hl: "en".into(),
        gl: "US".into(),
    }
}

#[tokio::test]
async fn signs_authenticated_browse_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .and(query_param("key", PLACEHOLDER))
        .and(query_param("prettyPrint", "false"))
        .and(header("x-origin", MUSIC_ORIGIN))
        .and(header("cookie", "SAPISID=test-sapisid; HSID=test-cookie"))
        .and(header_regex(
            "authorization",
            r"^SAPISIDHASH \d+_[0-9a-f]{40}$",
        ))
        .and(body_partial_json(json!({
            "browseId": "FEmusic_home",
            "context": { "client": { "clientName": "WEB_REMIX", "hl": "en", "gl": "US" } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&server)
        .await;

    let jar = CookieJar::from_header("SAPISID=test-sapisid; HSID=test-cookie");
    let client = InnerTubeClient::new(config_for(&server), Some(jar)).unwrap();
    assert_eq!(
        client.browse("FEmusic_home").await.unwrap(),
        json!({ "ok": true })
    );
}

#[tokio::test]
async fn unauthenticated_requests_carry_no_auth_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client.browse("FEmusic_home").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("authorization").is_none());
    assert!(requests[0].headers.get("cookie").is_none());
}

#[tokio::test]
async fn cookies_without_sapisid_fail_before_sending() {
    let server = MockServer::start().await;
    let jar = CookieJar::from_header("HSID=test-cookie");
    let client = InnerTubeClient::new(config_for(&server), Some(jar)).unwrap();
    assert!(matches!(
        client.browse("FEmusic_home").await,
        Err(YtmError::AuthExpired)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

async fn error_for_status(status: u16) -> YtmError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client.browse("FEmusic_home").await.unwrap_err()
}

#[tokio::test]
async fn maps_error_statuses() {
    assert!(matches!(error_for_status(401).await, YtmError::AuthExpired));
    assert!(matches!(error_for_status(403).await, YtmError::AuthExpired));
    assert!(matches!(error_for_status(429).await, YtmError::RateLimited));
    assert!(matches!(
        error_for_status(500).await,
        YtmError::Http { status: 500 }
    ));
}

#[tokio::test]
async fn network_error_omits_api_key() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let mut config = ClientConfig::music(PLACEHOLDER);
    config.base_url = format!("http://{address}");
    let client = InnerTubeClient::new(config, None).unwrap();
    let error = client.browse("FEmusic_home").await.unwrap_err();

    assert!(matches!(error, YtmError::Network(_)));
    assert!(!error.to_string().contains(PLACEHOLDER));
}

#[tokio::test]
async fn malformed_success_body_returns_parse_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    let error = client.browse("FEmusic_home").await.unwrap_err();

    assert!(matches!(
        error,
        YtmError::Parse { endpoint, path } if endpoint == "browse" && path == "<body>"
    ));
}

#[tokio::test]
async fn set_cookies_switches_auth_without_rebuilding_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client.browse("FEmusic_home").await.unwrap();
    client.set_cookies(Some(CookieJar::from_header("SAPISID=test-sapisid")));
    client.browse("FEmusic_home").await.unwrap();
    client.set_cookies(None);
    client.browse("FEmusic_home").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].headers.get("cookie").is_none());
    assert_eq!(
        requests[1].headers.get("cookie").unwrap(),
        "SAPISID=test-sapisid"
    );
    assert!(requests[1].headers.get("authorization").is_some());
    assert!(requests[2].headers.get("cookie").is_none());
}

#[tokio::test]
async fn shared_cookies_update_reaches_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let shared = SharedCookies::default();
    let client = InnerTubeClient::with_shared_cookies(config_for(&server), shared.clone()).unwrap();
    shared.set(Some(CookieJar::from_pairs([("SAPISID", "test-sapisid")])));
    client.browse("FEmusic_home").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("cookie").unwrap(),
        "SAPISID=test-sapisid"
    );
    assert!(client.cookies().get().is_some());
}

#[tokio::test]
async fn search_posts_the_query() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(query_param("key", PLACEHOLDER))
        .and(body_partial_json(json!({
            "query": "test query",
            "context": { "client": { "clientName": "WEB_REMIX" } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "contents": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client.search("test query").await.unwrap(),
        json!({ "contents": {} })
    );
}

#[tokio::test]
async fn radio_posts_the_seed_and_mix_settings() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({
            "videoId": "test-vid001",
            "playlistId": "RDAMVMtest-vid001",
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "contents": {} })))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client.radio("test-vid001").await.unwrap(),
        json!({ "contents": {} })
    );
}

#[tokio::test]
async fn radio_continuation_posts_the_token_and_panel_settings() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({
            "continuation": "test-continuation",
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "continuationContents": {} })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client
            .radio_continuation("test-continuation")
            .await
            .unwrap(),
        json!({ "continuationContents": {} })
    );
    let requests = server.received_requests().await.unwrap();
    let body = requests[0].body_json::<serde_json::Value>().unwrap();
    for key in ["videoId", "playlistId", "tunerSettingValue"] {
        assert!(body.get(key).is_none(), "unexpected key {key}");
    }
}

#[tokio::test]
async fn search_suggestions_post_the_input() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/music/get_search_suggestions"))
        .and(body_partial_json(json!({ "input": "test inp" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "contents": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert_eq!(
        client.search_suggestions("test inp").await.unwrap(),
        json!({ "contents": [] })
    );
}

#[tokio::test]
async fn search_maps_auth_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert!(matches!(
        client.search("test query").await,
        Err(YtmError::AuthExpired)
    ));
}

#[tokio::test]
async fn accounts_list_posts_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/account/accounts_list"))
        .and(body_partial_json(json!({
            "context": { "client": { "clientName": "WEB_REMIX" } }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "actions": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let jar = CookieJar::from_header("SAPISID=test-sapisid");
    let client = InnerTubeClient::new(config_for(&server), Some(jar)).unwrap();
    assert_eq!(
        client.accounts_list().await.unwrap(),
        json!({ "actions": [] })
    );
}

#[tokio::test]
async fn feed_browse_posts_optional_params_and_body_only_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(3)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client
        .browse_with_params("FEmusic_home", Some("test-chip%3D"))
        .await
        .unwrap();
    client
        .browse_with_params("test-category", None)
        .await
        .unwrap();
    client
        .browse_continuation("test-continuation")
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    for (request, expected) in requests.iter().zip([
        json!({"browseId": "FEmusic_home", "params": "test-chip%3D"}),
        json!({"browseId": "test-category"}),
        json!({"continuation": "test-continuation"}),
    ]) {
        let mut body = request.body_json::<serde_json::Value>().unwrap();
        assert!(body.as_object_mut().unwrap().remove("context").is_some());
        assert_eq!(body, expected);
        assert!(
            !request
                .url
                .query_pairs()
                .any(|(key, _)| key == "continuation")
        );
    }
}

#[tokio::test]
async fn artist_mix_posts_playlist_and_optional_params_without_a_video() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({
            "playlistId": "RD-test",
            "params": "test%3D",
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client
        .playback_queue(Some("RD-test"), None, Some("test%3D"))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body = requests[0].body_json::<serde_json::Value>().unwrap();
    assert!(body.get("videoId").is_none());
}

#[tokio::test]
async fn filtered_search_and_search_continuation_post_swift_request_shapes() {
    use ytm_core::models::SearchFilter;

    let filters = [
        (SearchFilter::Songs, "EgWKAQIIAWoMEA4QChADEAQQCRAF"),
        (SearchFilter::Videos, "EgWKAQIQAWoMEA4QChADEAQQCRAF"),
        (SearchFilter::Albums, "EgWKAQIYAWoMEA4QChADEAQQCRAF"),
        (SearchFilter::Artists, "EgWKAQIgAWoMEA4QChADEAQQCRAF"),
        (SearchFilter::Profiles, "EgWKAQJYAWoMEA4QChADEAQQCRAF"),
        (SearchFilter::Playlists, "EgWKAQIoAWoMEA4QChADEAQQCRAF"),
        (
            SearchFilter::FeaturedPlaylists,
            "EgeKAQQoADgBagwQDhAKEAMQBBAJEAU=",
        ),
        (
            SearchFilter::CommunityPlaylists,
            "EgeKAQQoAEABagwQDhAKEAMQBBAJEAU=",
        ),
        (
            SearchFilter::Podcasts,
            "EgWKAQJQAWoQEBAQCRAEEAMQBRAKEBUQEQ%3D%3D",
        ),
        (SearchFilter::Episodes, "EgWKAQJIAWoMEA4QChADEAQQCRAF"),
    ];
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(11)
        .mount(&server)
        .await;

    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    for (filter, _) in filters {
        client.search_filtered("test query", filter).await.unwrap();
    }
    client.search_continuation("test-token+/%3D").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let expected = filters
        .map(|(_, params)| json!({"query": "test query", "params": params}))
        .into_iter()
        .chain([json!({"continuation": "test-token+/%3D"})]);
    for (request, expected) in requests.iter().zip(expected) {
        let mut body = request.body_json::<serde_json::Value>().unwrap();
        assert!(body.as_object_mut().unwrap().remove("context").is_some());
        assert_eq!(body, expected);
        assert!(
            !request
                .url
                .query_pairs()
                .any(|(key, _)| key == "continuation")
        );
    }
}

#[tokio::test]
async fn lyrics_fetches_next_then_browse_for_plain_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({
            "videoId": "test-video",
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL"
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(include_str!("fixtures/lyrics_next.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .and(body_partial_json(json!({ "browseId": "MPLYt-test" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(include_str!("fixtures/lyrics_browse.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    let result = client.get_timed_lyrics("test-video").await.unwrap();
    assert_eq!(result.lines, None);
    assert_eq!(result.text.as_deref(), Some("First line\nSecond line"));
    assert_eq!(result.source.as_deref(), Some("Source: Test provider"));
}

#[tokio::test]
async fn task12_video_lyrics_description_keeps_timestamp_like_text_literal() {
    let text = "[00:01.00] First line";
    let result = fetch_lyrics_description(text).await;
    assert_eq!(result.lines, None);
    assert_eq!(result.text.as_deref(), Some(text));
    assert_eq!(result.source.as_deref(), Some("Test provider"));
}

#[tokio::test]
async fn task12_blank_lyrics_description_has_no_lyrics_or_attribution() {
    let result = fetch_lyrics_description("  \n  ").await;
    assert_eq!(result.lines, None);
    assert_eq!(result.text, None);
    assert_eq!(result.source, None);
}

async fn fetch_lyrics_description(text: &str) -> ytm_core::models::LyricsResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(include_str!("fixtures/lyrics_next.json")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "contents": {"sectionListRenderer": {"contents": [
                {"musicDescriptionShelfRenderer": {
                    "description": {"runs": [{"text": text}]},
                    "footer": {"runs": [{"text": "Test provider"}]}
                }}
            ]}}
        })))
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    client.get_timed_lyrics("test-video").await.unwrap()
}

#[tokio::test]
async fn lyrics_uses_timed_next_without_making_a_browse_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(include_str!("fixtures/lyrics_timed.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    let result = client.get_timed_lyrics("test-video").await.unwrap();
    assert_eq!(result.lines.unwrap()[0].start_ms, 1250);
    assert_eq!(result.text, None);
    assert_eq!(result.source.as_deref(), Some("YTMusic"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn account_action_requests_match_swift() {
    use ytm_core::models::{LikeStatus, PlaylistPrivacy};
    use ytm_core::secret::Secret;

    let server = MockServer::start().await;
    let client = InnerTubeClient::new(
        config_for(&server),
        Some(CookieJar::from_header("SAPISID=test-sapisid")),
    )
    .unwrap();
    Mock::given(method("POST"))
        .and(path("/playlist/create"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"playlistId": "PLtest"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    for rating in [
        LikeStatus::Like,
        LikeStatus::Dislike,
        LikeStatus::Indifferent,
    ] {
        client.rate_song("song-id", rating).await.unwrap();
    }
    client
        .edit_library(&Secret::new("test-feedback"))
        .await
        .unwrap();
    client.add_to_playlist_options("song-id").await.unwrap();
    client.add_to_playlist("song-id", "VLPLtest").await.unwrap();
    client
        .remove_from_playlist("VLPLtest", "song-id", "set-id")
        .await
        .unwrap();
    assert_eq!(
        client
            .create_playlist(
                "Title",
                "Description",
                PlaylistPrivacy::Unlisted,
                &["song-id".into()]
            )
            .await
            .unwrap(),
        "PLtest"
    );
    client
        .create_playlist("Empty", " ", PlaylistPrivacy::Private, &[])
        .await
        .unwrap();
    client.delete_playlist("VLPLtest").await.unwrap();
    client.set_artist_subscribed("UCtest", true).await.unwrap();
    client.set_artist_subscribed("UCtest", false).await.unwrap();
    client.set_playlist_saved("VLPLtest", true).await.unwrap();
    client.set_playlist_saved("VLPLtest", false).await.unwrap();

    let expected = [
        ("/like/like", json!({"target":{"videoId":"song-id"}})),
        ("/like/dislike", json!({"target":{"videoId":"song-id"}})),
        ("/like/removelike", json!({"target":{"videoId":"song-id"}})),
        ("/feedback", json!({"feedbackTokens":["test-feedback"]})),
        (
            "/playlist/get_add_to_playlist",
            json!({"videoIds":["song-id"]}),
        ),
        (
            "/browse/edit_playlist",
            json!({"playlistId":"PLtest", "actions":[{"action":"ACTION_ADD_VIDEO", "addedVideoId":"song-id"}]}),
        ),
        (
            "/browse/edit_playlist",
            json!({"playlistId":"PLtest", "actions":[{"action":"ACTION_REMOVE_VIDEO", "removedVideoId":"song-id","setVideoId":"set-id"}]}),
        ),
        (
            "/playlist/create",
            json!({"title":"Title","description":"Description","privacyStatus":"UNLISTED","videoIds":["song-id"]}),
        ),
        (
            "/playlist/create",
            json!({"title":"Empty","privacyStatus":"PRIVATE"}),
        ),
        ("/playlist/delete", json!({"playlistId":"PLtest"})),
        ("/subscription/subscribe", json!({"channelIds":["UCtest"]})),
        (
            "/subscription/unsubscribe",
            json!({"channelIds":["UCtest"]}),
        ),
        ("/like/like", json!({"target":{"playlistId":"PLtest"}})),
        (
            "/like/removelike",
            json!({"target":{"playlistId":"PLtest"}}),
        ),
    ];
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), expected.len());
    for (request, (path, expected_body)) in requests.iter().zip(expected) {
        assert_eq!(request.url.path(), path);
        let mut body: serde_json::Map<_, _> = serde_json::from_slice(&request.body).unwrap();
        assert!(body.remove("context").is_some());
        assert_eq!(json!(body), expected_body);
    }
}

#[tokio::test]
async fn song_metadata_posts_next_and_rejects_a_mismatched_video() {
    let server = MockServer::start().await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    let response: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/song_metadata.json")).unwrap();
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({
            "videoId": "song-id",
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/next"))
        .and(body_partial_json(json!({"videoId": "other-id"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .expect(1)
        .mount(&server)
        .await;
    assert!(client.song_metadata("song-id").await.unwrap().in_library);
    assert!(matches!(
        client.song_metadata("other-id").await,
        Err(YtmError::Parse { .. })
    ));
}

#[tokio::test]
async fn account_actions_require_auth_and_can_pin_the_request_cookies() {
    let server = MockServer::start().await;
    let client = InnerTubeClient::new(config_for(&server), None).unwrap();
    assert!(matches!(
        client.delete_playlist("PLtest").await,
        Err(YtmError::AuthExpired)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
    let pinned = client.with_cookie_snapshot(CookieJar::from_header("SAPISID=test-original"));
    client.set_cookies(Some(CookieJar::from_header("SAPISID=test-replacement")));
    Mock::given(method("POST"))
        .and(path("/playlist/delete"))
        .and(header("cookie", "SAPISID=test-original"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    pinned.delete_playlist("PLtest").await.unwrap();
}
