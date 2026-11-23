use std::collections::HashSet;

use serde_json::Value;

use super::home::{flex_column, parse_item};
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{
    AddToPlaylistMenu, AddToPlaylistOption, LikeStatus, PlaylistActions, PlaylistPrivacy,
    ShelfItem, SongMetadata,
};
use crate::secret::Secret;

const QUEUE_ITEM: &[&str] = &[
    "contents",
    "singleColumnMusicWatchNextResultsRenderer",
    "tabbedRenderer",
    "watchNextTabbedResultsRenderer",
    "tabs",
    "0",
    "tabRenderer",
    "content",
    "musicQueueRenderer",
    "content",
    "playlistPanelRenderer",
    "contents",
    "0",
];

pub fn parse_song_metadata(value: &Value) -> Result<SongMetadata, YtmError> {
    parse_song_metadata_for_video(value, None)
}

pub(crate) fn parse_song_metadata_for_video(
    value: &Value,
    expected_id: Option<&str>,
) -> Result<SongMetadata, YtmError> {
    let renderer = at(value, QUEUE_ITEM)
        .and_then(|item| {
            item.get("playlistPanelVideoRenderer")
                .filter(|value| value.is_object())
                .or_else(|| {
                    at(
                        item,
                        &[
                            "playlistPanelVideoWrapperRenderer",
                            "primaryRenderer",
                            "playlistPanelVideoRenderer",
                        ],
                    )
                    .filter(|value| value.is_object())
                })
        })
        .ok_or_else(|| YtmError::Parse {
            endpoint: "next".into(),
            path: "playlistPanelVideoRenderer".into(),
        })?;
    if let Some((actual, expected)) = renderer["videoId"].as_str().zip(expected_id)
        && actual != expected
    {
        return Err(YtmError::Parse {
            endpoint: "next".into(),
            path: "playlistPanelVideoRenderer.videoId".into(),
        });
    }
    let menu = &renderer["menu"]["menuRenderer"];
    let mut metadata = SongMetadata {
        like_status: array(&menu["topLevelButtons"])
            .iter()
            .find_map(|button| status(str_at(button, &["likeButtonRenderer", "likeStatus"])?)),
        in_library: false,
        add_token: None,
        remove_token: None,
    };

    for item in array(&menu["items"]) {
        if let Some(service) = item.get("menuServiceItemRenderer") {
            let token = feedback(service, "serviceEndpoint");
            match str_at(service, &["icon", "iconType"]) {
                Some("LIBRARY_ADD" | "BOOKMARK_BORDER") if token.is_some() => {
                    metadata.add_token = token;
                    metadata.remove_token = None;
                }
                Some("LIBRARY_REMOVE" | "BOOKMARK") => {
                    metadata.in_library = true;
                    if token.is_some() {
                        metadata.add_token = None;
                        metadata.remove_token = token;
                    }
                }
                _ => {}
            }
        }
        let Some(toggle) = item.get("toggleMenuServiceItemRenderer") else {
            continue;
        };
        let default = &toggle["defaultServiceEndpoint"];
        let toggled = &toggle["toggledServiceEndpoint"];
        if default.get("likeEndpoint").is_some() || toggled.get("likeEndpoint").is_some() {
            if metadata.like_status.is_none() {
                metadata.like_status = toggle_rating(toggle, renderer["videoId"].as_str());
            }
            continue;
        }
        let first = feedback(toggle, "defaultServiceEndpoint");
        let second = feedback(toggle, "toggledServiceEndpoint");
        match str_at(toggle, &["defaultIcon", "iconType"]) {
            Some("LIBRARY_ADD" | "BOOKMARK_BORDER") => {
                metadata.add_token = first;
                metadata.remove_token = second;
            }
            Some("LIBRARY_REMOVE" | "BOOKMARK") => {
                metadata.in_library = true;
                metadata.add_token = second;
                metadata.remove_token = first;
            }
            _ => {}
        }
    }
    Ok(metadata)
}

fn status(value: &str) -> Option<LikeStatus> {
    match value {
        "LIKE" => Some(LikeStatus::Like),
        "DISLIKE" => Some(LikeStatus::Dislike),
        "INDIFFERENT" => Some(LikeStatus::Indifferent),
        _ => None,
    }
}

fn feedback(item: &Value, key: &str) -> Option<Secret> {
    str_at(item, &[key, "feedbackEndpoint", "feedbackToken"])
        .filter(|token| !token.is_empty())
        .map(Secret::new)
}

fn toggle_rating(toggle: &Value, video_id: Option<&str>) -> Option<LikeStatus> {
    let default = &toggle["defaultServiceEndpoint"]["likeEndpoint"];
    let toggled = &toggle["toggledServiceEndpoint"]["likeEndpoint"];
    let (active, inactive) = if toggle["isToggled"].as_bool() == Some(true) {
        (toggled, default)
    } else {
        (default, toggled)
    };
    if active["status"].as_str() != Some("INDIFFERENT")
        || str_at(active, &["target", "videoId"])
            .zip(video_id)
            .is_some_and(|(a, b)| a != b)
    {
        return None;
    }
    Some(if inactive["status"].as_str() == Some("DISLIKE") {
        LikeStatus::Dislike
    } else {
        LikeStatus::Like
    })
}

pub fn parse_add_to_playlist(value: &Value) -> AddToPlaylistMenu {
    let renderer = find(value, |node| node.get("addToPlaylistRenderer")).unwrap_or(value);
    let mut options = Vec::new();
    let mut seen = HashSet::new();
    visit(renderer, &mut |node| {
        for key in [
            "playlistAddToOptionRenderer",
            "addToPlaylistItemRenderer",
            "musicResponsiveListItemRenderer",
            "musicTwoRowItemRenderer",
        ] {
            if let Some(option) = node
                .get(key)
                .and_then(|renderer| playlist_option(node, renderer))
                && seen.insert(
                    option
                        .playlist_id
                        .strip_prefix("VL")
                        .unwrap_or(&option.playlist_id)
                        .to_owned(),
                )
            {
                options.push(option);
            }
        }
    });
    AddToPlaylistMenu {
        title: text(&renderer["title"]),
        can_create_playlist: find(renderer, |node| node.get("createPlaylistEndpoint")).is_some(),
        options,
    }
}

fn playlist_option(wrapper: &Value, renderer: &Value) -> Option<AddToPlaylistOption> {
    if find(renderer, |node| node.get("createPlaylistEndpoint")).is_some() {
        return None;
    }
    let playlist_id = find(renderer, playlist_id)?.to_owned();
    let card = match parse_item(wrapper) {
        Some(ShelfItem::Playlist(card)) => Some(card),
        _ => None,
    };
    let title = ["title", "text", "label", "primaryText", "header"]
        .iter()
        .find_map(|key| text(&renderer[key]))
        .or_else(|| card.as_ref().map(|card| card.title.clone()))
        .or_else(|| flex_column(renderer, 0).and_then(text))
        .or_else(|| find(&renderer["runs"], |node| node["text"].as_str()).map(str::to_owned))
        .unwrap_or_else(|| "Unknown Playlist".into());
    if matches!(title.as_str(), "Create new playlist" | "New playlist") {
        return None;
    }
    let subtitle = text(&renderer["subtitle"])
        .or_else(|| text(&renderer["secondaryText"]))
        .or_else(|| card.as_ref().and_then(|card| card.subtitle.clone()));
    let explicit_privacy = [
        renderer["privacy"].as_str(),
        renderer["privacyStatus"].as_str(),
        subtitle.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_uppercase();
    let privacy_status = [
        ("PRIVATE", PlaylistPrivacy::Private),
        ("UNLISTED", PlaylistPrivacy::Unlisted),
        ("PUBLIC", PlaylistPrivacy::Public),
    ]
    .into_iter()
    .find_map(|(name, privacy)| explicit_privacy.contains(name).then_some(privacy));

    Some(AddToPlaylistOption {
        playlist_id,
        title,
        subtitle,
        privacy_status,
        thumbnail_url: thumbnail_url(renderer),
        is_selected: ["selected", "isSelected", "checked"]
            .iter()
            .find_map(|key| renderer[key].as_bool())
            .or_else(|| {
                renderer["checkStatus"].as_str().map(|status| {
                    let status = status.to_uppercase();
                    !["UNCHECK", "UNSELECTED", "NOT_SELECTED"]
                        .iter()
                        .any(|value| status.contains(value))
                        && ["CHECKED", "SELECTED"]
                            .iter()
                            .any(|value| status.contains(value))
                })
            })
            .or_else(|| renderer["toggled"].as_bool())
            .unwrap_or(false),
    })
}

pub(super) fn playlist_actions(value: &Value, header: Option<&Value>) -> PlaylistActions {
    let can_delete = find(value, |node| {
        node.get("deletePlaylistEndpoint")
            .or_else(|| node.get("musicEditablePlaylistDetailHeaderRenderer"))
            .or_else(|| {
                node.as_str()
                    .filter(|text| *text == "playlist/delete")
                    .map(|_| node)
            })
    })
    .is_some();
    let saved = header.and_then(|header| {
        find(header, |node| {
            let toggle = node.get("toggleButtonRenderer");
            if let Some(toggle) = toggle {
                let icon = str_at(toggle, &["defaultIcon", "iconType"]);
                let state = toggle["isToggled"].as_bool()?;
                return match icon {
                    Some("LIBRARY_ADD" | "BOOKMARK_BORDER") => Some(state),
                    Some("LIBRARY_REMOVE" | "BOOKMARK" | "LIBRARY_SAVED") => Some(!state),
                    _ => None,
                };
            }
            let like = node.get("likeButtonRenderer")?;
            str_at(like, &["target", "playlistId"])?;
            Some(status(like["likeStatus"].as_str()?)? == LikeStatus::Like)
        })
    });
    PlaylistActions { can_delete, saved }
}

pub(super) fn playlist_set_video_id(renderer: &Value) -> Option<String> {
    nonempty(str_at(
        renderer,
        &["playlistItemData", "playlistSetVideoId"],
    ))
    .or_else(|| {
        find(renderer, |node| {
            array(at(node, &["playlistEditEndpoint", "actions"])?)
                .iter()
                .find_map(|action| {
                    (action["action"].as_str() == Some("ACTION_REMOVE_VIDEO"))
                        .then(|| nonempty(action["setVideoId"].as_str()))
                        .flatten()
                })
        })
    })
    .map(str::to_owned)
}

pub(crate) fn created_playlist_id(value: &Value) -> Option<&str> {
    nonempty(value["playlistId"].as_str())
        .or_else(|| {
            let actions = array(&value["actions"]);
            actions
                .iter()
                .find_map(|action| {
                    nonempty(str_at(
                        action,
                        &[
                            "addToToastAction",
                            "item",
                            "notificationTextRenderer",
                            "navigationEndpoint",
                            "browseEndpoint",
                            "playlistId",
                        ],
                    ))
                })
                .or_else(|| {
                    actions.iter().find_map(|action| {
                        nonempty(str_at(
                            action,
                            &["navigationEndpoint", "browseEndpoint", "playlistId"],
                        ))
                    })
                })
        })
        .or_else(|| nonempty(str_at(value, &["command", "browseEndpoint", "playlistId"])))
        .or_else(|| nonempty(str_at(value, &["command", "playlistId"])))
        .or_else(|| find(value, playlist_id))
}

fn playlist_id(node: &Value) -> Option<&str> {
    nonempty(node["playlistId"].as_str()).or_else(|| {
        nonempty(node["browseId"].as_str())
            .filter(|id| id.starts_with("VL") || id.starts_with("PL"))
    })
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn text(node: &Value) -> Option<String> {
    node["simpleText"]
        .as_str()
        .or_else(|| node["content"].as_str())
        .map(str::to_owned)
        .or_else(|| runs_text(node))
}

fn array(node: &Value) -> &[Value] {
    node.as_array().map_or(&[], Vec::as_slice)
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node.as_object()
        .into_iter()
        .flat_map(|object| object.values())
        .chain(array(node))
}

fn find<'a, T>(node: &'a Value, extract: impl Fn(&'a Value) -> Option<T>) -> Option<T> {
    fn walk<'a, T>(node: &'a Value, extract: &impl Fn(&'a Value) -> Option<T>) -> Option<T> {
        extract(node).or_else(|| children(node).find_map(|child| walk(child, extract)))
    }
    walk(node, &extract)
}

fn visit(node: &Value, action: &mut impl FnMut(&Value)) {
    action(node);
    for child in children(node) {
        visit(child, action);
    }
}
