use std::future::ready;

use clap::Parser;
use kaset_session::{MemoryStore, SessionStore};
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use ytm_core::client::ClientConfig;
use ytm_core::secret::Secret;

use super::*;

const SEARCH: &str = include_str!("../../ytm-core/tests/fixtures/search_response.json");
const FEED: &str = include_str!("../../ytm-core/tests/fixtures/feed_response.json");
const RADIO: &str = include_str!("../../ytm-core/tests/fixtures/radio_response.json");

fn page_fixture() -> Value {
    json!({
        "header": {"musicDetailHeaderRenderer": {
            "title": {"runs": [{"text": "Test Album"}]},
            "subtitle": {"runs": [{"text": "Test Artist"}]},
            "secondSubtitle": {"runs": [{"text": "1 song"}]}
        }},
        "contents": {"singleColumnBrowseResultsRenderer": {"tabs": [{"tabRenderer": {
            "content": {"sectionListRenderer": {"contents": [{"musicShelfRenderer": {
                "contents": [{"musicResponsiveListItemRenderer": {
                    "playlistItemData": {"videoId": "test-vid001"},
                    "flexColumns": [
                        {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Test Track"}]}}},
                        {"musicResponsiveListItemFlexColumnRenderer": {"text": {"runs": [{"text": "Test Artist"}]}}}
                    ]
                }}]
            }}]}}
        }}]}}
    })
}

fn account_fixture() -> Value {
    json!({"actions": [{"getMultiPageMenuAction": {"menu": {"multiPageMenuRenderer": {
        "sections": [{"accountSectionListRenderer": {"contents": [
            {"accountItemSectionRenderer": {"contents": [{"accountItem": {
                "accountName": {"runs": [{"text": "Test User"}]},
                "channelHandle": {"runs": [{"text": "@testuser"}]},
                "isSelected": true
            }}]}}
        ]}}]
    }}}}]})
}

#[test]
fn parses_search_options_and_global_flags() {
    let args = Cli::try_parse_from([
        "kaset",
        "--json",
        "search",
        "test query",
        "--filter",
        "videos",
        "--limit",
        "2",
    ])
    .unwrap();
    assert!(args.json);
    #[cfg(debug_assertions)]
    assert_eq!(
        Cli::try_parse_from(["kaset", "whoami", "--session-file", "test-session.txt"])
            .unwrap()
            .session_file,
        Some("test-session.txt".into())
    );
    assert!(matches!(
        args.command,
        Command::Api(ApiCommand::Search {
            query, filter: Some(args::SearchFilter::Videos), limit: Some(2)
        }) if query == "test query"
    ));
    for invalid in [
        vec!["search"],
        vec!["search", "query", "--filter", "episodes"],
        vec!["search", "query", "--limit", "-1"],
        vec!["page", "bad/id"],
        vec!["radio", "bad-id"],
    ] {
        assert!(Cli::try_parse_from(std::iter::once("kaset").chain(invalid)).is_err());
    }
}

#[cfg(debug_assertions)]
#[test]
fn selects_explicit_file_then_env_file_then_default_keyring() {
    assert_eq!(
        session_file(Some("explicit".into()), Some("file"), Some("env".into())).unwrap(),
        Some("explicit".into())
    );
    assert_eq!(
        session_file(None, Some("file"), Some("env".into())).unwrap(),
        Some("env".into())
    );
    assert_eq!(
        session_file(None, Some("keyring"), Some("env".into())).unwrap(),
        None
    );
    assert_eq!(session_file(None, None, None).unwrap(), None);
    assert!(session_file(None, Some("file"), None).is_err());
    assert!(session_file(None, Some("file"), Some("".into())).is_err());
}

fn mock_client(server: &MockServer, jar: CookieJar) -> InnerTubeClient {
    let mut config = ClientConfig::music("test-key");
    config.base_url = server.uri();
    InnerTubeClient::new(config, Some(jar)).unwrap()
}

fn stored_session() -> MemoryStore {
    let store = MemoryStore::default();
    store.save(&Secret::new("SAPISID=test-sapisid")).unwrap();
    store
}

#[tokio::test]
async fn maps_missing_and_http_expired_sessions_to_documented_exit_codes() {
    let missing = MemoryStore::default();
    let result = run(
        &missing,
        ApiCommand::Whoami,
        |_, _| -> std::future::Ready<Result<Output, YtmError>> {
            panic!("missing session must short-circuit the client")
        },
    )
    .await;
    let Err(error) = result else {
        panic!("expected missing session error")
    };
    assert_eq!(error.exit_code(), ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        "Not signed in. Sign in with the Kaset app first."
    );

    let store = stored_session();
    let saved = store.load().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/account/accounts_list"))
        .respond_with(ResponseTemplate::new(403).set_body_string("test-private-body"))
        .expect(1)
        .mount(&server)
        .await;
    let result = run(&store, ApiCommand::Whoami, |jar, command| async {
        execute(&mock_client(&server, jar), command).await
    })
    .await;
    let Err(error) = result else {
        panic!("expected expired session error")
    };
    assert_eq!(error.exit_code(), ExitCode::from(3));
    assert_eq!(
        error.report(),
        "Session expired. Open the Kaset app to sign in again."
    );
    assert_eq!(store.load().unwrap(), saved);

    store.save(&Secret::new("SID=test-only")).unwrap();
    let result = run(&store, ApiCommand::Whoami, |_, _| {
        ready(Ok(Output::Account(None)))
    })
    .await;
    assert!(matches!(result, Err(CliError::Api(YtmError::AuthExpired))));
}

#[tokio::test]
async fn routes_and_formats_every_standalone_command() {
    let search = serde_json::from_str(SEARCH).unwrap();
    let feed: Value = serde_json::from_str(FEED).unwrap();
    let radio = serde_json::from_str(RADIO).unwrap();
    let cases = [
        (
            vec!["search", "test query", "--filter", "songs", "--limit", "1"],
            "/search",
            json!({"query": "test query"}),
            &search,
            "Song  Search Result Song 1 — Search Artist 1  [search-video-1]\n",
            "/items/0/video_id",
            "search-video-1",
        ),
        (
            vec!["home"],
            "/browse",
            json!({"browseId": "FEmusic_home"}),
            &feed,
            "Genres\n  Category  Jazz  [FEmusic_moods_and_genres_category]\n  Category  Blues  [test-category]\n",
            "/sections/0/items/0/browse_id",
            "FEmusic_moods_and_genres_category",
        ),
        (
            vec!["explore"],
            "/browse",
            json!({"browseId": "FEmusic_explore"}),
            &feed,
            "",
            "/sections/0/title",
            "Genres",
        ),
        (
            vec!["charts"],
            "/browse",
            json!({"browseId": "FEmusic_charts"}),
            &feed,
            "",
            "/sections/0/title",
            "Genres",
        ),
        (
            vec!["moods"],
            "/browse",
            json!({"browseId": "FEmusic_moods_and_genres"}),
            &feed,
            "",
            "/sections/0/title",
            "Genres",
        ),
        (
            vec!["new"],
            "/browse",
            json!({"browseId": "FEmusic_new_releases"}),
            &feed,
            "",
            "/sections/0/title",
            "Genres",
        ),
        (
            vec!["history"],
            "/browse",
            json!({"browseId": "FEmusic_history"}),
            &feed,
            "",
            "/sections/0/title",
            "Genres",
        ),
        (
            vec!["page", "MPREb_test"],
            "/browse",
            json!({"browseId": "MPREb_test"}),
            &page_fixture(),
            "Test Album\nTest Artist\n1 song\n\n1. Test Track — Test Artist  [test-vid001]\n",
            "/songs/0/video_id",
            "test-vid001",
        ),
        (
            vec!["radio", "test-vid001"],
            "/next",
            json!({"videoId": "test-vid001"}),
            &radio,
            "Up next\n1. Seed song — First artist, Second artist, Third artist  [test-vid001]\n2. Wrapped song  [test-vid002]\n",
            "/songs/1/video_id",
            "test-vid002",
        ),
        (
            vec!["whoami"],
            "/account/accounts_list",
            json!({}),
            &account_fixture(),
            "Test User (@testuser)\n",
            "/handle",
            "@testuser",
        ),
    ];

    let server = MockServer::start().await;
    let client = mock_client(&server, CookieJar::from_header("SAPISID=test-sapisid"));
    for (args, endpoint, body, response, expected, json_pointer, json_value) in cases {
        let mock = Mock::given(method("POST"))
            .and(path(endpoint))
            .and(body_partial_json(body))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let command = Cli::try_parse_from(std::iter::once("kaset").chain(args))
            .unwrap()
            .command;
        let Command::Api(command) = command else {
            panic!("expected an API command");
        };
        let output = execute(&client, command).await.unwrap();
        let mut text = Vec::new();
        output.write(&mut text, false, false).unwrap();
        let text = String::from_utf8(text).unwrap();
        if expected.is_empty() {
            assert!(text.starts_with("Genres\n"));
        } else {
            assert_eq!(text, expected);
        }
        let mut bytes = Vec::new();
        output.write(&mut bytes, true, true).unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.pointer(json_pointer), Some(&json!(json_value)));
        let serialized = String::from_utf8(bytes).unwrap();
        for value in [
            "test-continuation",
            "test-chip%3D",
            "test-jazz",
            "test-sapisid",
        ] {
            assert!(!serialized.contains(value));
        }
        drop(mock);
    }
}

#[test]
fn terminal_dim_only_wraps_ids() {
    let queue =
        ytm_core::parsers::radio::parse_radio(&serde_json::from_str(RADIO).unwrap()).unwrap();
    let mut text = Vec::new();
    Output::Radio(queue).write(&mut text, false, true).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains(
        "Seed song — First artist, Second artist, Third artist  \x1b[2m[test-vid001]\x1b[0m"
    ));
}

#[test]
fn task12_empty_no_color_keeps_terminal_styling() {
    use std::ffi::OsStr;

    assert!(dim_ids(true, None));
    assert!(dim_ids(true, Some(OsStr::new(""))));
    assert!(!dim_ids(true, Some(OsStr::new("1"))));
    assert!(!dim_ids(false, None));
}

#[cfg(not(debug_assertions))]
#[test]
fn task12_release_ignores_file_store_environment() {
    assert_eq!(
        session_file(None, Some("file"), Some("test-session".into())).unwrap(),
        None
    );
}

#[test]
fn task13_page_and_play_accept_the_same_browse_id_boundaries() {
    for id in ["x", &"a".repeat(256)] {
        assert!(Cli::try_parse_from(["kaset", "page", id]).is_ok());
        assert!(Cli::try_parse_from(["kaset", "play", "--page", id]).is_ok());
    }
    for id in ["", "bad/id", "가", &"a".repeat(257)] {
        assert!(Cli::try_parse_from(["kaset", "page", id]).is_err());
        assert!(Cli::try_parse_from(["kaset", "play", "--page", id]).is_err());
    }
}

#[tokio::test]
async fn task13_page_normalizes_raw_playlist_ids() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .and(body_partial_json(json!({"browseId": "VLPL-test"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(page_fixture()))
        .expect(1)
        .mount(&server)
        .await;
    let client = mock_client(&server, CookieJar::from_header("SAPISID=test-sapisid"));
    let output = execute(
        &client,
        ApiCommand::Page {
            browse_id: "PL-test".into(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(output, Output::Page(_)));
}

#[tokio::test(start_paused = true)]
async fn task13_session_read_retries_corruption_only() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::Instant;

    struct RotatingStore {
        attempts: AtomicUsize,
        failures: usize,
        corrupt: bool,
    }

    impl SessionStore for RotatingStore {
        fn load(&self) -> Result<Option<Secret>, StoreError> {
            if self.attempts.fetch_add(1, Ordering::Relaxed) < self.failures {
                return if self.corrupt {
                    Err(StoreError::Corrupt("missing chunk"))
                } else {
                    Err(StoreError::Keyring("unavailable"))
                };
            }
            Ok(Some(Secret::new("SAPISID=test-sapisid")))
        }

        fn save(&self, _: &Secret) -> Result<(), StoreError> {
            panic!("read must not write the store");
        }

        fn clear(&self) -> Result<(), StoreError> {
            panic!("read must not clear the store");
        }
    }

    for (failures, corrupt, attempts, delay, succeeds) in [
        (1, true, 2, 50, true),
        (3, true, 4, 150, true),
        (4, true, 4, 150, false),
        (1, false, 1, 0, false),
    ] {
        let store = RotatingStore {
            attempts: AtomicUsize::new(0),
            failures,
            corrupt,
        };
        let start = Instant::now();
        let result = run(&store, ApiCommand::Whoami, |_, _| {
            ready(Ok(Output::Account(None)))
        })
        .await;
        assert_eq!(result.is_ok(), succeeds);
        assert_eq!(store.attempts.load(Ordering::Relaxed), attempts);
        assert_eq!(start.elapsed(), Duration::from_millis(delay));
    }
}
