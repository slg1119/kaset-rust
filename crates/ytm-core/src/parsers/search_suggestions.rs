//! `music/get_search_suggestions` responses, ported from the Swift
//! `SearchSuggestionsParser`.

use std::collections::HashSet;

use serde_json::Value;

/// Text and history suggestions in order, without duplicates. Rich
/// suggestions (songs, artists) are skipped, as in the Swift app.
pub fn parse_search_suggestions(response: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    let sections = response
        .get("contents")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    sections
        .iter()
        .filter_map(|section| {
            section
                .get("searchSuggestionsSectionRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|item| {
            let renderer = item
                .get("searchSuggestionRenderer")
                .or_else(|| item.get("historySuggestionRenderer"))?;
            let text: String = renderer
                .get("suggestion")?
                .get("runs")?
                .as_array()?
                .iter()
                .filter_map(|run| run.get("text")?.as_str())
                .collect();
            (!text.is_empty()).then_some(text)
        })
        .filter(|text| seen.insert(text.clone()))
        .collect()
}
