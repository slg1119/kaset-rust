use serde_json::{Value, json};
use ytm_core::models::Account;
use ytm_core::parsers::account::parse_account;

fn item(name: &str, handle: Option<&str>, selected: bool) -> Value {
    let mut item = json!({
        "accountName": { "runs": [{ "text": name }] },
        "accountPhoto": { "thumbnails": [
            { "url": "//yt3.ggpht.com/test-small=s48" },
            { "url": "//yt3.ggpht.com/test-large=s88" }
        ] },
        "isSelected": selected
    });
    if let Some(handle) = handle {
        item["channelHandle"] = json!({ "runs": [{ "text": handle }] });
    }
    json!({ "accountItem": item })
}

fn response(items: Vec<Value>) -> Value {
    json!({ "actions": [{ "getMultiPageMenuAction": { "menu": { "multiPageMenuRenderer": {
        "sections": [
            { "accountSectionListRenderer": {
                "header": { "googleAccountHeaderRenderer": {
                    "email": { "runs": [{ "text": "test@example.com" }] } } },
                "contents": [{ "accountItemSectionRenderer": { "contents": items } }]
            } }
        ]
    } } } }] })
}

#[test]
fn picks_the_selected_account() {
    let account = parse_account(&response(vec![
        item("Brand Account", Some("@brandaccount"), false),
        item("Test User", Some("@testuser"), true),
    ]));
    assert_eq!(
        account,
        Some(Account {
            name: "Test User".into(),
            handle: Some("@testuser".into()),
            photo_url: Some("https://yt3.ggpht.com/test-large=s88".into()),
        })
    );
}

#[test]
fn falls_back_to_the_first_account() {
    let account = parse_account(&response(vec![
        item("Test User", None, false),
        item("Brand Account", Some("@brandaccount"), false),
    ]))
    .unwrap();
    assert_eq!(account.name, "Test User");
    assert_eq!(account.handle, None);
}

#[test]
fn skips_nameless_items_and_empty_responses() {
    assert_eq!(parse_account(&json!({})), None);
    assert_eq!(parse_account(&json!({ "actions": [] })), None);
    assert_eq!(parse_account(&response(vec![item("", None, true)])), None);
}
