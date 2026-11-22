use serde_json::{Value, json};
use ytm_core::parsers::search_suggestions::parse_search_suggestions;

fn suggestion(kind: &str, runs: Value) -> Value {
    json!({ kind: { "suggestion": { "runs": runs } } })
}

#[test]
fn suggestions_keep_order_and_drop_duplicates() {
    let response = json!({ "contents": [
        { "searchSuggestionsSectionRenderer": { "contents": [
            suggestion("historySuggestionRenderer", json!([{ "text": "taylor swift" }])),
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor" }, { "text": " swift songs" }])),
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor swift" }])),
            suggestion("searchSuggestionRenderer", json!([]))
        ] } },
        { "searchSuggestionsSectionRenderer": { "contents": [
            { "musicResponsiveListItemRenderer": {} },
            suggestion("searchSuggestionRenderer", json!([{ "text": "taylor swift playlist" }]))
        ] } }
    ] });
    assert_eq!(
        parse_search_suggestions(&response),
        [
            "taylor swift",
            "taylor swift songs",
            "taylor swift playlist"
        ]
    );
}

#[test]
fn suggestions_without_contents_are_empty() {
    assert!(parse_search_suggestions(&json!({})).is_empty());
    assert!(parse_search_suggestions(&json!({ "contents": [] })).is_empty());
}
