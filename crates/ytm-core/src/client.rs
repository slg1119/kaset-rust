use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use crate::YtmError;
use crate::auth::{CookieJar, sapisid_hash};

pub const MUSIC_ORIGIN: &str = "https://music.youtube.com";
pub const CLIENT_VERSION: &str = "1.20231204.01.00";
pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub api_key: String,
    pub hl: String,
    pub gl: String,
}

impl ClientConfig {
    pub fn music(api_key: impl Into<String>) -> Self {
        Self {
            base_url: format!("{MUSIC_ORIGIN}/youtubei/v1"),
            api_key: api_key.into(),
            hl: "en".into(),
            gl: "US".into(),
        }
    }
}

/// YouTube Music InnerTube client (`WEB_REMIX`).
pub struct InnerTubeClient {
    http: reqwest::Client,
    config: ClientConfig,
    cookies: Option<CookieJar>,
}

impl InnerTubeClient {
    pub fn new(config: ClientConfig, cookies: Option<CookieJar>) -> Result<Self, YtmError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            config,
            cookies,
        })
    }

    fn context(&self) -> Value {
        json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": CLIENT_VERSION,
                "hl": self.config.hl,
                "gl": self.config.gl,
                "platform": "DESKTOP",
                "userAgent": USER_AGENT,
                "utcOffsetMinutes": chrono::Local::now().offset().local_minus_utc() / 60,
            },
            "user": { "lockedSafetyMode": false }
        })
    }

    pub async fn post(
        &self,
        endpoint: &str,
        mut body: Map<String, Value>,
    ) -> Result<Value, YtmError> {
        body.insert("context".into(), self.context());
        let mut request = self
            .http
            .post(format!("{}/{endpoint}", self.config.base_url))
            .query(&[
                ("key", self.config.api_key.as_str()),
                ("prettyPrint", "false"),
            ])
            .header("Origin", MUSIC_ORIGIN)
            .header("X-Origin", MUSIC_ORIGIN)
            .header("Referer", MUSIC_ORIGIN)
            .json(&body);

        if let Some(jar) = &self.cookies {
            let sapisid = jar.sapisid().ok_or(YtmError::AuthExpired)?;
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let hash = sapisid_hash(sapisid.expose(), MUSIC_ORIGIN, timestamp);
            request = request
                .header("Cookie", jar.header().expose())
                .header("Authorization", format!("SAPISIDHASH {hash}"))
                .header("X-Goog-AuthUser", "0");
        }

        let response = request
            .send()
            .await
            .map_err(|error| YtmError::Network(error.without_url()))?;
        match response.status().as_u16() {
            200..=299 => {
                let body = response
                    .text()
                    .await
                    .map_err(|error| YtmError::Network(error.without_url()))?;
                serde_json::from_str(&body).map_err(|_| YtmError::Parse {
                    endpoint: endpoint.to_owned(),
                    path: "<body>".into(),
                })
            }
            401 | 403 => Err(YtmError::AuthExpired),
            429 => Err(YtmError::RateLimited),
            status => Err(YtmError::Http { status }),
        }
    }

    pub async fn browse(&self, browse_id: &str) -> Result<Value, YtmError> {
        let mut body = Map::new();
        body.insert("browseId".into(), browse_id.into());
        self.post("browse", body).await
    }
}
