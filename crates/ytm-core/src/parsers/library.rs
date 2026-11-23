use serde_json::Value;

use super::helpers::{append_actions, shelf_continuation};
use super::home::{SECTION_LIST, parse_item};
use crate::YtmError;
use crate::json::{at, runs_text};
use crate::models::{HomeSection, LibraryPage, ShelfItem};

/// Parses the library landing or a dedicated collection.
pub fn parse_library(value: &Value) -> Result<LibraryPage, YtmError> {
    let list = at(value, SECTION_LIST).ok_or_else(parse_error)?;
    parse_section_list(list)
}

pub fn parse_library_continuation(value: &Value) -> Result<LibraryPage, YtmError> {
    let mut page = empty_page();
    if let Some(container) = value.get("continuationContents") {
        if let Some(list) = container.get("sectionListContinuation") {
            return parse_section_list(list);
        }
        for (renderer, rows_key) in [
            ("gridContinuation", "items"),
            ("musicShelfContinuation", "contents"),
        ] {
            if let Some(shelf) = container.get(renderer)
                && append_shelf(&mut page, shelf, rows_key)
            {
                return Ok(page);
            }
        }
    }
    let mut actions = append_actions(value).peekable();
    if actions.peek().is_none() {
        return Err(parse_error());
    }
    for (action, rows) in actions {
        append_items(&mut page, action, rows);
    }
    Ok(page)
}

fn parse_section_list(list: &Value) -> Result<LibraryPage, YtmError> {
    let rows = list
        .get("contents")
        .and_then(Value::as_array)
        .ok_or_else(parse_error)?;
    let mut page = empty_page();
    for section in rows {
        for (renderer, rows_key) in [
            ("gridRenderer", "items"),
            ("musicShelfRenderer", "contents"),
        ] {
            if let Some(shelf) = section.get(renderer) {
                append_shelf(&mut page, shelf, rows_key);
            }
        }
        if let Some(children) =
            at(section, &["itemSectionRenderer", "contents"]).and_then(Value::as_array)
        {
            for shelf in children
                .iter()
                .filter_map(|child| child.get("musicShelfRenderer"))
            {
                append_shelf(&mut page, shelf, "contents");
            }
        }
    }
    append_token(&mut page, list, rows);
    Ok(page)
}

fn append_shelf(page: &mut LibraryPage, shelf: &Value, key: &str) -> bool {
    let Some(rows) = shelf.get(key).and_then(Value::as_array) else {
        return false;
    };
    append_items(page, shelf, rows);
    true
}

fn append_items(page: &mut LibraryPage, shelf: &Value, rows: &[Value]) {
    let items: Vec<_> = rows
        .iter()
        .filter_map(parse_item)
        .filter(|item| {
            matches!(
                item,
                ShelfItem::Album(_) | ShelfItem::Artist(_) | ShelfItem::Playlist(_)
            )
        })
        .collect();
    if !items.is_empty() {
        let title = at(shelf, &["header", "gridHeaderRenderer", "title"])
            .or_else(|| shelf.get("title"))
            .and_then(runs_text)
            .unwrap_or_else(|| "Library".into());
        page.sections.push(HomeSection {
            title,
            items,
            more: None,
        });
    }
    append_token(page, shelf, rows);
}

fn append_token(page: &mut LibraryPage, shelf: &Value, rows: &[Value]) {
    if let Some(token) = shelf_continuation(shelf, rows)
        && !page.continuations.iter().any(|known| known == token)
    {
        page.continuations.push(token.into());
    }
}

fn empty_page() -> LibraryPage {
    LibraryPage {
        sections: Vec::new(),
        continuations: Vec::new(),
    }
}

fn parse_error() -> YtmError {
    YtmError::Parse {
        endpoint: "browse".into(),
        path: "library sections".into(),
    }
}
