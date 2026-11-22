use serde_json::Value;

use super::home::{SECTION_LIST, parse_section};
use crate::YtmError;
use crate::json::{at, runs_text, str_at, thumbnail_url};
use crate::models::{ArtistPage, BrowseEndpoint, HomeSection, ShelfItem};

pub fn parse_artist(response: &Value) -> Result<ArtistPage, YtmError> {
    let (header, name) = ["musicImmersiveHeaderRenderer", "musicVisualHeaderRenderer"]
        .into_iter()
        .find_map(|key| {
            let header = at(response, &["header", key])?;
            Some((header, runs_text(header.get("title")?)?))
        })
        .ok_or_else(|| parse_error("header"))?;

    let subscription = at(header, &["subscriptionButton", "subscribeButtonRenderer"]);
    let radio = watch_endpoint(header, "startRadioButton");
    let shuffle =
        watch_endpoint(header, "shuffleButton").or_else(|| watch_endpoint(header, "playButton"));
    let mut sections = Vec::new();
    let mut top_songs_section = None;
    for section in artist_sections(response).unwrap_or_default() {
        let Some(parsed) = artist_section(section) else {
            continue;
        };
        if top_songs_section.is_none()
            && section.get("musicShelfRenderer").is_some()
            && parsed
                .items
                .iter()
                .all(|item| matches!(item, ShelfItem::Song(_)))
        {
            top_songs_section = u32::try_from(sections.len()).ok();
        }
        sections.push(parsed);
    }
    Ok(ArtistPage {
        name,
        description: header.get("description").and_then(runs_text),
        thumbnail_url: thumbnail_url(header),
        subscribers: subscription.and_then(|button| {
            ["shortSubscriberCountText", "subscriberCountText"]
                .into_iter()
                .find_map(|key| button.get(key).and_then(runs_text))
        }),
        channel_id: string_field(subscription, "channelId").or_else(|| {
            str_at(
                response,
                &["microformat", "microformatDataRenderer", "urlCanonical"],
            )?
            .rsplit_once("/channel/")
            .map(|(_, id)| id.to_owned())
        }),
        subscribed: subscription
            .and_then(|button| button.get("subscribed"))
            .and_then(Value::as_bool)
            .or_else(|| menu_subscription(header)),
        radio_playlist_id: string_field(radio, "playlistId"),
        radio_video_id: string_field(radio, "videoId"),
        radio_params: string_field(radio, "params"),
        shuffle_playlist_id: string_field(shuffle, "playlistId"),
        shuffle_video_id: string_field(shuffle, "videoId"),
        shuffle_params: string_field(shuffle, "params"),
        top_songs_section,
        sections,
    })
}

pub fn parse_artist_items(response: &Value) -> Result<Vec<ShelfItem>, YtmError> {
    let sections =
        artist_sections(response).ok_or_else(|| parse_error("sectionListRenderer.contents"))?;
    Ok(sections
        .iter()
        .filter_map(parse_section)
        .flat_map(|section| section.items)
        .collect())
}

fn artist_sections(response: &Value) -> Option<&[Value]> {
    Some(at(response, SECTION_LIST)?.get("contents")?.as_array()?)
}

fn artist_section(section: &Value) -> Option<HomeSection> {
    let mut parsed = parse_section(section)?;
    let button = at(
        section,
        &[
            "musicCarouselShelfRenderer",
            "header",
            "musicCarouselShelfBasicHeaderRenderer",
            "moreContentButton",
            "buttonRenderer",
        ],
    );
    let endpoint = at(
        section,
        &["musicShelfRenderer", "bottomEndpoint", "browseEndpoint"],
    )
    .or_else(|| at(button?, &["command", "browseEndpoint"]))
    .or_else(|| at(button?, &["navigationEndpoint", "browseEndpoint"]));
    parsed.more = string_field(endpoint, "browseId").map(|browse_id| BrowseEndpoint {
        browse_id,
        params: string_field(endpoint, "params"),
    });
    Some(parsed)
}

fn watch_endpoint<'a>(header: &'a Value, button: &str) -> Option<&'a Value> {
    let endpoint = at(header, &[button, "buttonRenderer", "navigationEndpoint"])?;
    endpoint
        .get("watchPlaylistEndpoint")
        .or_else(|| endpoint.get("watchEndpoint"))
}

fn string_field(value: Option<&Value>, field: &str) -> Option<String> {
    str_at(value?, &[field])
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn menu_subscription(header: &Value) -> Option<bool> {
    at(header, &["menu", "menuRenderer", "items"])?
        .as_array()?
        .iter()
        .find_map(|item| {
            match str_at(
                item,
                &["toggleMenuServiceItemRenderer", "defaultIcon", "iconType"],
            )? {
                "SUBSCRIBED" | "NOTIFICATION_ON" => Some(true),
                "SUBSCRIBE" | "NOTIFICATION_OFF" => Some(false),
                _ => None,
            }
        })
}

fn parse_error(path: &str) -> YtmError {
    YtmError::Parse {
        endpoint: "browse:artist".into(),
        path: path.into(),
    }
}
