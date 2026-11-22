use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use ytm_core::YtmError;
use ytm_core::api_key::{extract_api_key, fetch_api_key};

#[test]
fn extracts_key_from_ytcfg() {
    let html = r#"<script>ytcfg.set({"INNERTUBE_API_KEY": "test-api-key","OTHER":1})</script>"#;
    assert_eq!(extract_api_key(html).as_deref(), Some("test-api-key"));
}

#[test]
fn missing_key_returns_none() {
    assert_eq!(extract_api_key("<html></html>"), None);
}

#[tokio::test]
async fn fetch_sends_consent_cookie_and_parses_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .and(header("cookie", "SOCS=CAI"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#""INNERTUBE_API_KEY":"test-api-key""#),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        fetch_api_key(&format!("{}/", server.uri())).await.unwrap(),
        "test-api-key"
    );
}

#[tokio::test]
async fn fetch_without_key_is_parse_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>consent</html>"))
        .mount(&server)
        .await;
    let err = fetch_api_key(&format!("{}/", server.uri()))
        .await
        .unwrap_err();
    assert!(matches!(err, YtmError::Parse { .. }), "{err:?}");
}
