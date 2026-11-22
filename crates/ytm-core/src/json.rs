use serde_json::Value;

/// Walks `path`; numeric segments index into arrays.
pub(crate) fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, key| match key.parse::<usize>() {
            Ok(index) if current.is_array() => current.get(index),
            _ => current.get(*key),
        })
}

pub(crate) fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    at(value, path)?.as_str()
}

/// Concatenates `runs[].text`. Returns `None` when empty.
pub(crate) fn runs_text(value: &Value) -> Option<String> {
    let text: String = value
        .get("runs")?
        .as_array()?
        .iter()
        .filter_map(|run| run.get("text")?.as_str())
        .collect();
    (!text.is_empty()).then_some(text)
}

/// Last non-empty thumbnail URL in the first non-empty renderer thumbnail list.
pub(crate) fn thumbnail_url(item: &Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &[
            "thumbnail",
            "musicThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
        &[
            "thumbnail",
            "croppedSquareThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
        &["thumbnail", "thumbnails"],
        &[
            "thumbnailRenderer",
            "musicThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
        &[
            "thumbnailRenderer",
            "croppedSquareThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
        &[
            "foregroundThumbnail",
            "musicThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
        &["thumbnails"],
    ];

    let thumbnails = PATHS
        .iter()
        .filter_map(|path| at(item, path)?.as_array())
        .find(|thumbnails| !thumbnails.is_empty())?;
    let url = thumbnails
        .iter()
        .rev()
        .filter_map(|thumbnail| thumbnail.get("url")?.as_str())
        .find(|url| !url.is_empty())?;
    if url.starts_with("//") {
        Some(format!("https:{url}"))
    } else if let Some(rest) = url.strip_prefix("http://") {
        Some(format!("https://{rest}"))
    } else {
        Some(url.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::thumbnail_url;
    use serde_json::json;

    const ART: &str = "https://lh3.googleusercontent.com/test-art";
    const SMALL_ART: &str = "https://i.ytimg.com/vi/test/hq.jpg";

    #[test]
    fn reads_music_thumbnail() {
        let item = json!({
            "thumbnail": {
                "musicThumbnailRenderer": {
                    "thumbnail": {"thumbnails": [{"url": SMALL_ART}, {"url": ART}]}
                }
            }
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_cropped_square_thumbnail() {
        let item = json!({
            "thumbnail": {
                "croppedSquareThumbnailRenderer": {
                    "thumbnail": {"thumbnails": [{"url": ART}]}
                }
            }
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_direct_thumbnail_list() {
        let item = json!({"thumbnail": {"thumbnails": [{"url": ART}]}});
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_music_thumbnail_from_thumbnail_renderer() {
        let item = json!({
            "thumbnailRenderer": {
                "musicThumbnailRenderer": {
                    "thumbnail": {"thumbnails": [{"url": ART}]}
                }
            }
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_cropped_square_thumbnail_from_thumbnail_renderer() {
        let item = json!({
            "thumbnailRenderer": {
                "croppedSquareThumbnailRenderer": {
                    "thumbnail": {"thumbnails": [{"url": ART}]}
                }
            }
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_foreground_thumbnail() {
        let item = json!({
            "foregroundThumbnail": {
                "musicThumbnailRenderer": {
                    "thumbnail": {"thumbnails": [{"url": ART}]}
                }
            }
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn reads_top_level_thumbnail_list() {
        let item = json!({"thumbnails": [{"url": ART}]});
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn normalizes_protocol_relative_url() {
        let item = json!({"thumbnails": [{"url": "//lh3.googleusercontent.com/test-art"}]});
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn upgrades_http_url() {
        let item = json!({"thumbnails": [{"url": "http://lh3.googleusercontent.com/test-art"}]});
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn returns_none_for_empty_list() {
        let item = json!({"thumbnails": []});
        assert_eq!(thumbnail_url(&item), None);
    }

    #[test]
    fn uses_first_non_empty_list_and_skips_empty_urls() {
        let item = json!({
            "thumbnail": {
                "musicThumbnailRenderer": {"thumbnail": {"thumbnails": []}},
                "thumbnails": [{"url": ART}, {"url": ""}]
            },
            "thumbnails": [{"url": SMALL_ART}]
        });
        assert_eq!(thumbnail_url(&item).as_deref(), Some(ART));
    }

    #[test]
    fn returns_none_when_first_non_empty_list_has_no_url() {
        let item = json!({
            "thumbnail": {"thumbnails": [{"url": ""}]},
            "thumbnails": [{"url": ART}]
        });
        assert_eq!(thumbnail_url(&item), None);
    }
}
