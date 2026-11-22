use std::net::TcpListener;

use serde_json::json;
use wiremock::matchers::{body_partial_json, header, header_regex, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use ytm_core::YtmError;
use ytm_core::auth::{CookieJar, SharedCookies};
use ytm_core::client::{ClientConfig, InnerTubeClient, MUSIC_ORIGIN};

fn config_for(server: &MockServer) -> ClientConfig {
    ClientConfig {
        base_url: server.uri(),
        api_key: "test-api-key".into(),
        hl: "en".into(),
        gl: "US".into(),
    }
}

#[tokio::test]
async fn signs_authenticated_browse_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/browse"))
        .and(query_param("key", "test-api-key"))
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

    let mut config = ClientConfig::music("test-api-key");
    config.base_url = format!("http://{address}");
    let client = InnerTubeClient::new(config, None).unwrap();
    let error = client.browse("FEmusic_home").await.unwrap_err();

    assert!(matches!(error, YtmError::Network(_)));
    assert!(!error.to_string().contains("test-api-key"));
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
