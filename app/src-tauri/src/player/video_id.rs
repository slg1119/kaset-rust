use serde::{Deserialize, Deserializer, Serialize};
use tauri::Url;

/// A YouTube video id: exactly 11 characters from `[A-Za-z0-9_-]`. Checked
/// before any id reaches a URL or the player, whether it came from the UI,
/// the API, or the player page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(transparent)]
pub struct VideoId(String);

impl VideoId {
    pub const LEN: usize = 11;

    pub fn parse(value: &str) -> Option<Self> {
        let valid = value.len() == Self::LEN
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid.then(|| Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `https://music.youtube.com/watch?v=<id>`.
    pub fn watch_url(&self) -> Url {
        let mut url = Url::parse("https://music.youtube.com/watch").expect("constant URL is valid");
        url.query_pairs_mut().append_pair("v", &self.0);
        url
    }
}

impl<'de> Deserialize<'de> for VideoId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("invalid video id"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_eleven_url_safe_characters() {
        for id in ["dQw4w9WgXcQ", "abc-DEF_123", "___________", "-----------"] {
            assert_eq!(VideoId::parse(id).map(|id| id.0), Some(id.to_owned()));
        }
    }

    #[test]
    fn rejects_everything_else() {
        for id in [
            "",
            "short",
            "dQw4w9WgXcQx",
            "dQw4w9WgXc!",
            "dQw4w9WgXc ",
            "dQw4w9WgX&v",
            "dQw4w9WgX#a",
            "dQw4w9WgX/a",
            "dQw4w9WgXcé",
            "../../etc/p",
        ] {
            assert!(VideoId::parse(id).is_none(), "{id:?}");
        }
    }

    #[test]
    fn builds_the_watch_url() {
        let id = VideoId::parse("abc-DEF_123").unwrap();
        assert_eq!(
            id.watch_url().as_str(),
            "https://music.youtube.com/watch?v=abc-DEF_123"
        );
    }

    #[test]
    fn deserializing_validates() {
        assert_eq!(
            serde_json::from_str::<VideoId>("\"abc-DEF_123\"").unwrap(),
            VideoId::parse("abc-DEF_123").unwrap()
        );
        assert!(serde_json::from_str::<VideoId>("\"javascript:x\"").is_err());
        assert!(serde_json::from_str::<VideoId>("42").is_err());
    }
}
