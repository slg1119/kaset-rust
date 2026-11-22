use std::sync::LazyLock;

use regex::Regex;

use crate::YtmError;
use crate::client::{CONNECT_TIMEOUT, REQUEST_TIMEOUT, USER_AGENT};

/// Environment variable that overrides the fetched key.
pub const API_KEY_ENV: &str = "KASET_YTMUSIC_API_KEY";

static KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""INNERTUBE_API_KEY"\s*:\s*"([^"]+)""#).expect("static regex is valid")
});

pub fn extract_api_key(html: &str) -> Option<String> {
    KEY_PATTERN.captures(html).map(|c| c[1].to_owned())
}

/// Fetches the public web client page and extracts the InnerTube key.
/// Sends no user cookies: a stale consent cookie lands on consent.youtube.com,
/// which has no key. `SOCS=CAI` pre-accepts consent.
pub async fn fetch_api_key(web_url: &str) -> Result<String, YtmError> {
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let response = http
        .get(web_url)
        .header("Cookie", "SOCS=CAI")
        .send()
        .await
        .map_err(|error| YtmError::Network(error.without_url()))?;
    let status = response.status().as_u16();
    if !(200..400).contains(&status) {
        return Err(YtmError::Http { status });
    }
    let html = response
        .text()
        .await
        .map_err(|error| YtmError::Network(error.without_url()))?;
    extract_api_key(&html).ok_or_else(|| YtmError::Parse {
        endpoint: "web-client".into(),
        path: "INNERTUBE_API_KEY".into(),
    })
}
