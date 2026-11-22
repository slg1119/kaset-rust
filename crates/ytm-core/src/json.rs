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

/// Largest (last) thumbnail of a music item renderer.
pub(crate) fn thumbnail_url(item: &Value) -> Option<String> {
    at(
        item,
        &[
            "thumbnail",
            "musicThumbnailRenderer",
            "thumbnail",
            "thumbnails",
        ],
    )?
    .as_array()?
    .last()?
    .get("url")?
    .as_str()
    .map(str::to_owned)
}
