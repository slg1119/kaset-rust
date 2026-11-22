//! `account/accounts_list` responses, reduced from the Swift
//! `AccountsListParser` to what the sidebar shows. The Google email and
//! brand-account tokens are deliberately not read.

use serde_json::Value;

use crate::json::{at, str_at};
use crate::models::Account;

const SECTIONS: &[&str] = &[
    "actions",
    "0",
    "getMultiPageMenuAction",
    "menu",
    "multiPageMenuRenderer",
    "sections",
];

/// The selected account, or the first listed one; `None` when the response
/// lists no account with a name.
pub fn parse_account(response: &Value) -> Option<Account> {
    let accounts: Vec<(bool, Account)> = at(response, SECTIONS)?
        .as_array()?
        .iter()
        .filter_map(|section| {
            section
                .get("accountSectionListRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|content| {
            content
                .get("accountItemSectionRenderer")?
                .get("contents")?
                .as_array()
        })
        .flatten()
        .filter_map(|wrapper| parse_item(wrapper.get("accountItem")?))
        .collect();
    let selected = accounts.iter().position(|(selected, _)| *selected);
    accounts
        .into_iter()
        .nth(selected.unwrap_or(0))
        .map(|(_, account)| account)
}

fn parse_item(item: &Value) -> Option<(bool, Account)> {
    let name =
        str_at(item, &["accountName", "runs", "0", "text"]).filter(|name| !name.is_empty())?;
    let handle = str_at(item, &["channelHandle", "runs", "0", "text"]).map(str::to_owned);
    let photo_url = item
        .get("accountPhoto")
        .and_then(|photo| photo.get("thumbnails"))
        .and_then(Value::as_array)
        .and_then(|thumbnails| thumbnails.last())
        .and_then(|thumbnail| thumbnail.get("url"))
        .and_then(Value::as_str)
        .map(|url| match url.strip_prefix("//") {
            Some(rest) => format!("https://{rest}"),
            None => url.to_owned(),
        });
    let selected = item
        .get("isSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some((
        selected,
        Account {
            name: name.to_owned(),
            handle,
            photo_url,
        },
    ))
}
