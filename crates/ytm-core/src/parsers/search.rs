//! `search` responses. Ported from the Swift `SearchResponseParser`: results
//! keep server order, the first occurrence of each item wins, and browse
//! semantics (album, artist, playlist, …) win over watch fallbacks.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::helpers::{
    artists_from_flex_column, is_artist_page_type, is_clock_duration, is_navigable_artist_id,
    page_type,
};
use crate::json::{at, str_at, thumbnail_url};
use crate::models::{ArtistRef, BrowseCard, SearchItem, SearchResults, Song};

/// Parses the first page of a search. A response without a section list is
/// an empty result, as in the Swift parser (YouTube returns one for queries
/// with no matches).
pub fn parse_search(response: &Value) -> SearchResults {
    let mut results = Accumulator::default();
    if let Some(shelf) = at(response, &["contents", "musicShelfRenderer"]) {
        parse_shelf(shelf, &mut results);
        return results.finish();
    }
    let lists = first_page_section_lists(response);
    if lists.is_empty() {
        tracing::debug!("search response has no section list");
    }
    for list in lists {
        for content in array(list.get("contents")) {
            parse_container(content, &mut results);
        }
        results.capture(continuation_token(list));
    }
    results.finish()
}

/// Parses a search continuation from a legacy shelf or a response action.
pub fn parse_search_continuation(response: &Value) -> SearchResults {
    let mut results = Accumulator::default();
    if let Some(shelf) = at(
        response,
        &["continuationContents", "musicShelfContinuation"],
    ) {
        parse_shelf(shelf, &mut results);
    }
    for envelope in [
        "onResponseReceivedCommands",
        "onResponseReceivedActions",
        "onResponseReceivedEndpoints",
    ] {
        for action in array(response.get(envelope)) {
            for command in [
                "appendContinuationItemsAction",
                "reloadContinuationItemsCommand",
            ] {
                for item in array(at(action, &[command, "continuationItems"])) {
                    parse_container(item, &mut results);
                }
            }
        }
    }
    results.finish()
}

#[derive(Default)]
struct Accumulator {
    items: Vec<SearchItem>,
    seen: HashSet<String>,
    continuation: Option<String>,
}

impl Accumulator {
    fn finish(self) -> SearchResults {
        SearchResults {
            items: self.items,
            continuation: self.continuation,
        }
    }

    fn push(&mut self, item: SearchItem) {
        if self.seen.insert(identity(&item)) {
            self.items.push(item);
        }
    }

    fn capture(&mut self, token: Option<&str>) {
        if self.continuation.is_some() {
            return;
        }
        if let Some(token) = token.map(str::trim).filter(|token| !token.is_empty()) {
            self.continuation = Some(token.to_owned());
        }
    }
}

/// Songs, videos, and episodes share the video namespace, so a video listed
/// twice under different kinds appears once.
fn identity(item: &SearchItem) -> String {
    match item {
        SearchItem::Song(song) | SearchItem::Video(song) | SearchItem::Episode(song) => {
            format!("video:{}", song.video_id)
        }
        SearchItem::Album(card) => format!("album:{}", card.browse_id),
        SearchItem::Artist(card) | SearchItem::Profile(card) => {
            format!("artist:{}", card.browse_id)
        }
        SearchItem::Playlist(card) => {
            let id = card.browse_id.as_str();
            format!("playlist:{}", id.strip_prefix("VL").unwrap_or(id))
        }
        SearchItem::Podcast(card) => format!("podcast:{}", card.browse_id),
    }
}

fn array(value: Option<&Value>) -> &[Value] {
    value.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn first_page_section_lists(response: &Value) -> Vec<&Value> {
    let Some(contents) = response.get("contents") else {
        return Vec::new();
    };
    if let Some(list) = contents.get("sectionListRenderer") {
        return vec![list];
    }
    array(at(contents, &["tabbedSearchResultsRenderer", "tabs"]))
        .iter()
        .filter_map(|tab| at(tab, &["tabRenderer", "content", "sectionListRenderer"]))
        .collect()
}

fn parse_container(container: &Value, results: &mut Accumulator) {
    if let Some(card) = container.get("musicCardShelfRenderer") {
        if let Some(item) = parse_renderer(card, &card_endpoints(card)) {
            results.push(item);
        }
        for content in array(card.get("contents")) {
            parse_container(content, results);
        }
        results.capture(continuation_token(card));
        return;
    }
    if let Some(shelf) = container.get("musicShelfRenderer") {
        parse_shelf(shelf, results);
        return;
    }
    if let Some(section) = container.get("itemSectionRenderer") {
        for content in array(section.get("contents")) {
            parse_container(content, results);
        }
        return;
    }
    let item = if let Some(renderer) = container.get("musicResponsiveListItemRenderer") {
        parse_renderer(renderer, &responsive_endpoints(renderer))
    } else if let Some(renderer) = container.get("musicTwoRowItemRenderer") {
        parse_renderer(renderer, &two_row_endpoints(renderer))
    } else if let Some(renderer) = container.get("musicMultiRowListItemRenderer") {
        parse_renderer(renderer, &multi_row_endpoints(renderer))
    } else {
        results.capture(continuation_item_token(container));
        return;
    };
    match item {
        Some(item) => results.push(item),
        None => tracing::debug!("skipping unparsed search result"),
    }
}

fn parse_shelf(shelf: &Value, results: &mut Accumulator) {
    results.capture(continuation_token(shelf));
    for content in array(shelf.get("contents")) {
        parse_container(content, results);
    }
}

fn continuation_token(renderer: &Value) -> Option<&str> {
    for continuation in array(renderer.get("continuations")) {
        for key in ["nextContinuationData", "reloadContinuationData"] {
            if let Some(token) =
                str_at(continuation, &[key, "continuation"]).filter(|token| !token.is_empty())
            {
                return Some(token);
            }
        }
    }
    array(renderer.get("contents"))
        .iter()
        .find_map(continuation_item_token)
}

fn continuation_item_token(item: &Value) -> Option<&str> {
    str_at(
        item,
        &[
            "continuationItemRenderer",
            "continuationEndpoint",
            "continuationCommand",
            "token",
        ],
    )
    .filter(|token| !token.is_empty())
}

// MARK: - Endpoints

/// An endpoint, unwrapped from `innertubeCommand` when present.
fn unwrap_endpoint(value: Option<&Value>) -> Option<&Value> {
    let endpoint = value.filter(|value| value.is_object())?;
    Some(endpoint.get("innertubeCommand").unwrap_or(endpoint))
}

fn play_endpoint(overlay: Option<&Value>) -> Option<&Value> {
    unwrap_endpoint(at(
        overlay?,
        &[
            "musicItemThumbnailOverlayRenderer",
            "content",
            "musicPlayButtonRenderer",
            "playNavigationEndpoint",
        ],
    ))
}

fn title_run(renderer: &Value) -> Option<&Value> {
    let flex_title = at(
        renderer,
        &[
            "flexColumns",
            "0",
            "musicResponsiveListItemFlexColumnRenderer",
            "text",
            "runs",
            "0",
        ],
    );
    flex_title.or_else(|| at(renderer, &["title", "runs", "0"]))
}

fn collect(candidates: [Option<&Value>; 5]) -> Vec<&Value> {
    candidates
        .into_iter()
        .filter_map(|candidate| unwrap_endpoint(candidate))
        .collect()
}

fn responsive_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        renderer.get("onTap"),
        play_endpoint(renderer.get("overlay")),
        None,
    ])
}

fn two_row_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        renderer.get("onTap"),
        play_endpoint(renderer.get("thumbnailOverlay")),
        play_endpoint(renderer.get("overlay")),
    ])
}

fn multi_row_endpoints(renderer: &Value) -> Vec<&Value> {
    collect([
        renderer.get("onTap"),
        renderer.get("navigationEndpoint"),
        title_run(renderer).and_then(|run| run.get("navigationEndpoint")),
        play_endpoint(renderer.get("overlay")),
        None,
    ])
}

fn card_endpoints(card: &Value) -> Vec<&Value> {
    collect([
        title_run(card).and_then(|run| run.get("navigationEndpoint")),
        card.get("navigationEndpoint"),
        card.get("onTap"),
        play_endpoint(card.get("thumbnailOverlay")),
        play_endpoint(card.get("overlay")),
    ])
}

// MARK: - Items

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentKind {
    Song,
    Video,
    Album,
    Audiobook,
    Artist,
    Profile,
    Playlist,
    PodcastShow,
    PodcastEpisode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowseKind {
    Album,
    Artist,
    Profile,
    Playlist,
    PodcastShow,
    PodcastEpisode,
}

fn parse_renderer(renderer: &Value, endpoints: &[&Value]) -> Option<SearchItem> {
    let title = item_title(renderer).unwrap_or_else(|| "Unknown".into());
    let thumbnail = thumbnail_url(renderer);
    let kind = content_kind(renderer);

    let mut episode_browse = false;
    for endpoint in endpoints {
        let Some(browse) = endpoint.get("browseEndpoint") else {
            continue;
        };
        let Some(browse_id) = str_at(browse, &["browseId"]) else {
            continue;
        };
        let Some(browse_kind) = browse_kind(browse_id, page_type(browse), kind) else {
            continue;
        };
        if browse_kind == BrowseKind::PodcastEpisode {
            episode_browse = true;
            continue;
        }
        return Some(browse_item(
            browse_kind,
            browse_id,
            title,
            thumbnail,
            renderer,
        ));
    }

    for endpoint in endpoints {
        let watch_playlist = str_at(endpoint, &["watchPlaylistEndpoint", "playlistId"]);
        let contextual = (kind == Some(ContentKind::Playlist))
            .then(|| str_at(endpoint, &["watchEndpoint", "playlistId"]))
            .flatten();
        if let Some(id) = watch_playlist
            .or(contextual)
            .filter(|id| is_playlist_id(id))
        {
            return Some(browse_item(
                BrowseKind::Playlist,
                id,
                title,
                thumbnail,
                renderer,
            ));
        }
    }

    let (video_id, video_type) = playable(renderer, endpoints)?;
    let song = Song {
        video_id: video_id.to_owned(),
        title,
        artists: Vec::new(),
        thumbnail_url: thumbnail,
    };
    if episode_browse
        || kind == Some(ContentKind::PodcastEpisode)
        || video_type == Some("MUSIC_VIDEO_TYPE_PODCAST_EPISODE")
    {
        return Some(SearchItem::Episode(Song {
            artists: episode_show(renderer).into_iter().collect(),
            ..song
        }));
    }
    let song = Song {
        artists: item_artists(renderer, true),
        ..song
    };
    let is_video = kind == Some(ContentKind::Video)
        || matches!(
            video_type,
            Some(
                "MUSIC_VIDEO_TYPE_OMV"
                    | "MUSIC_VIDEO_TYPE_UGC"
                    | "MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC"
            )
        );
    Some(if is_video {
        SearchItem::Video(song)
    } else {
        SearchItem::Song(song)
    })
}

fn browse_item(
    kind: BrowseKind,
    browse_id: &str,
    title: String,
    thumbnail_url: Option<String>,
    renderer: &Value,
) -> SearchItem {
    let card = BrowseCard {
        browse_id: browse_id.to_owned(),
        title,
        subtitle: semantic_subtitle(renderer),
        thumbnail_url,
    };
    match kind {
        BrowseKind::Album => SearchItem::Album(card),
        BrowseKind::Artist => SearchItem::Artist(card),
        BrowseKind::Profile => SearchItem::Profile(card),
        BrowseKind::Playlist => SearchItem::Playlist(card),
        // Episodes never reach here: they become playable items.
        BrowseKind::PodcastShow | BrowseKind::PodcastEpisode => SearchItem::Podcast(card),
    }
}

fn is_playlist_id(id: &str) -> bool {
    ["VL", "PL", "VM", "RD"]
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// pageType first, then browseId prefix, as in the Swift parser.
fn browse_kind(
    browse_id: &str,
    page_type: Option<&str>,
    content: Option<ContentKind>,
) -> Option<BrowseKind> {
    match page_type {
        Some("MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK") => {
            return Some(BrowseKind::Album);
        }
        Some("MUSIC_PAGE_TYPE_USER_CHANNEL") => return Some(BrowseKind::Profile),
        page_type if is_artist_page_type(page_type) => return Some(BrowseKind::Artist),
        Some("MUSIC_PAGE_TYPE_PLAYLIST") => return Some(BrowseKind::Playlist),
        Some("MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE") => return Some(BrowseKind::PodcastShow),
        _ => {}
    }
    if browse_id.starts_with("MPSPP") {
        Some(BrowseKind::PodcastShow)
    } else if browse_id.starts_with("MPED") || content == Some(ContentKind::PodcastEpisode) {
        Some(BrowseKind::PodcastEpisode)
    } else if browse_id.starts_with("MPRE") || browse_id.starts_with("OLAK") {
        Some(BrowseKind::Album)
    } else if is_playlist_id(browse_id) {
        Some(BrowseKind::Playlist)
    } else if is_navigable_artist_id(browse_id) {
        Some(if content == Some(ContentKind::Profile) {
            BrowseKind::Profile
        } else {
            BrowseKind::Artist
        })
    } else {
        None
    }
}

/// The video id and music video type: `playlistItemData` first, then the
/// first endpoint with a `watchEndpoint`.
fn playable<'a>(
    renderer: &'a Value,
    endpoints: &[&'a Value],
) -> Option<(&'a str, Option<&'a str>)> {
    let renderer_type = video_type(renderer.get("navigationEndpoint"))
        .or_else(|| video_type(play_endpoint(renderer.get("overlay"))));
    let first_type = renderer_type.or_else(|| {
        endpoints
            .iter()
            .find_map(|endpoint| video_type(Some(endpoint)))
    });
    if let Some(id) = str_at(renderer, &["playlistItemData", "videoId"]).filter(|id| !id.is_empty())
    {
        return Some((id, first_type));
    }
    endpoints.iter().find_map(|endpoint| {
        let id = str_at(endpoint, &["watchEndpoint", "videoId"]).filter(|id| !id.is_empty())?;
        Some((id, video_type(Some(endpoint)).or(first_type)))
    })
}

fn video_type(endpoint: Option<&Value>) -> Option<&str> {
    str_at(
        endpoint?,
        &[
            "watchEndpoint",
            "watchEndpointMusicSupportedConfigs",
            "watchEndpointMusicConfig",
            "musicVideoType",
        ],
    )
}

fn item_title(renderer: &Value) -> Option<String> {
    let text = title_run(renderer)?.get("text")?.as_str()?;
    Some(text.to_owned())
}

// MARK: - Metadata

/// Runs of the flex columns after the title, or else `subtitle` and
/// `secondSubtitle`.
fn metadata_runs(renderer: &Value) -> Vec<&Value> {
    let flex: Vec<&Value> = array(renderer.get("flexColumns"))
        .iter()
        .skip(1)
        .flat_map(|column| {
            array(at(
                column,
                &["musicResponsiveListItemFlexColumnRenderer", "text", "runs"],
            ))
        })
        .collect();
    if !flex.is_empty() {
        return flex;
    }
    ["subtitle", "secondSubtitle"]
        .iter()
        .flat_map(|key| array(at(renderer, &[key, "runs"])))
        .collect()
}

fn split_metadata(text: &str) -> impl Iterator<Item = &str> {
    text.split(['•', '·'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
}

fn content_kind(renderer: &Value) -> Option<ContentKind> {
    metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| run.get("text")?.as_str())
        .flat_map(split_metadata)
        .find_map(content_kind_label)
}

/// Metadata parts that are not content type labels.
fn metadata_components(renderer: &Value) -> Vec<String> {
    metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| run.get("text")?.as_str())
        .flat_map(split_metadata)
        .filter(|part| content_kind_label(part).is_none())
        .map(str::to_owned)
        .collect()
}

fn semantic_subtitle(renderer: &Value) -> Option<String> {
    let components = metadata_components(renderer);
    (!components.is_empty()).then(|| components.join(" • "))
}

/// Flex-column artists first, then linked artist runs, then (for songs and
/// videos) the first plain metadata part that is not a count, date, or
/// duration.
fn item_artists(renderer: &Value, plain_text_fallback: bool) -> Vec<ArtistRef> {
    if renderer.get("flexColumns").is_some() {
        let column = at(
            renderer,
            &[
                "flexColumns",
                "1",
                "musicResponsiveListItemFlexColumnRenderer",
                "text",
            ],
        );
        let artists: Vec<ArtistRef> = artists_from_flex_column(column)
            .into_iter()
            .filter(|artist| {
                artist.browse_id.is_some() || content_kind_label(&artist.name).is_none()
            })
            .collect();
        if !artists.is_empty() {
            return artists;
        }
    }
    let linked: Vec<ArtistRef> = metadata_runs(renderer)
        .into_iter()
        .filter_map(|run| {
            let name = run.get("text")?.as_str()?.trim();
            let browse = unwrap_endpoint(run.get("navigationEndpoint"))?.get("browseEndpoint")?;
            let id = str_at(browse, &["browseId"])?;
            (!name.is_empty()
                && (is_navigable_artist_id(id) || is_artist_page_type(page_type(browse))))
            .then(|| ArtistRef {
                name: name.to_owned(),
                browse_id: Some(id.to_owned()),
            })
        })
        .collect();
    if !linked.is_empty() || !plain_text_fallback {
        return linked;
    }
    metadata_components(renderer)
        .into_iter()
        .find(|part| !is_non_artist_metadata(part))
        .map(|name| ArtistRef {
            name,
            browse_id: None,
        })
        .into_iter()
        .collect()
}

/// The show an episode links to (`MPSPP…` browse id).
fn episode_show(renderer: &Value) -> Option<ArtistRef> {
    metadata_runs(renderer).into_iter().find_map(|run| {
        let browse = unwrap_endpoint(run.get("navigationEndpoint"))?.get("browseEndpoint")?;
        let id = str_at(browse, &["browseId"]).filter(|id| id.starts_with("MPSPP"))?;
        let name = run.get("text")?.as_str()?.trim();
        (!name.is_empty()).then(|| ArtistRef {
            name: name.to_owned(),
            browse_id: Some(id.to_owned()),
        })
    })
}

static NON_ARTIST_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)^\s*\d+(?:[.,]\d+)?\s*(?:seconds?|secs?|minutes?|mins?|hours?|hrs?)\s*$",
        r"(?i)^\s*\d+\s*(?:seconds?|secs?|minutes?|mins?|hours?|hrs?|days?|weeks?|months?|years?|[smhdwy])\s+ago\s*$",
        r"(?i)^\s*(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+\d{1,2}(?:,\s*\d{4})?\s*$",
        r"^\s*(?:\d{4}|\d{4}-\d{1,2}-\d{1,2})\s*$",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("static regex is valid"))
    .collect()
});

fn is_non_artist_metadata(text: &str) -> bool {
    content_kind_label(text).is_some()
        || is_clock_duration(text.trim())
        || has_count_unit(text)
        || NON_ARTIST_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(text))
}

/// "12 songs", "42K subscribers", "1.2M views", and localized variants.
fn has_count_unit(text: &str) -> bool {
    let normalized = text
        .replace(['\u{00A0}', '\u{202F}'], " ")
        .trim()
        .to_lowercase();
    if !normalized.chars().any(char::is_numeric) {
        return false;
    }
    let Some(unit) = normalized
        .split(|ch: char| ch.is_whitespace() || ch.is_numeric() || ch == '.' || ch == ',')
        .rfind(|part| !part.is_empty())
    else {
        return false;
    };
    COUNT_UNITS.contains(&unit)
        || ["выпуск", "odcin", "епізод"]
            .iter()
            .any(|prefix| unit.starts_with(prefix))
}

// Kept in sync with SearchResponseParser+Support.swift `countMetadataUnits`.
#[rustfmt::skip]
const COUNT_UNITS: &[&str] = &[
    "subscriber", "subscribers", "view", "views", "play", "plays", "episode", "episodes", "song", "songs",
    "track", "tracks", "حلقة", "الحلقات", "أغنية", "أغاني", "أغانٍ", "مقطوعات", "folge", "folgen",
    "titel", "titeln", "episodio", "episodios", "canción", "canciones", "pista", "pistas", "épisode",
    "épisodes", "morceau", "morceaux", "titre", "titres", "lagu", "trek", "episodi", "brano", "brani",
    "에피소드", "노래", "곡", "트랙", "aflevering", "afleveringen", "nummer", "nummers", "odcinek",
    "odcinki", "utwór", "utwory", "utworów", "utworami", "episódio", "episódios", "música", "músicas",
    "faixa", "faixas", "выпуск", "выпуски", "трек", "треки", "треков", "avsnitt", "låt", "låtar",
    "spår", "bölüm", "bölümler", "şarkı", "şarkılar", "parça", "епізод", "епізоди", "пісня",
    "пісні", "пісень", "треків",
];

// Kept in sync with SearchResponseParser.swift `contentKind(fromLabel:)`.
fn content_kind_label(label: &str) -> Option<ContentKind> {
    let label = label.trim().to_lowercase();
    let kind = match label.as_str() {
        "song" | "أغنية" | "titel" | "canción" | "morceau" | "lagu" | "brano" | "노래"
        | "nummer" | "utwór" | "música" | "трек" | "låt" | "şarkı" | "пісня" => {
            ContentKind::Song
        }
        "video" | "فيديو" | "vídeo" | "vidéo" | "동영상" | "wideo" | "видео" | "відео" => {
            ContentKind::Video
        }
        "album" | "single" | "ep" | "ألبوم" | "álbum" | "앨범" | "альбом" | "albüm" => {
            ContentKind::Album
        }
        "audiobook"
        | "كتاب صوتي"
        | "hörbuch"
        | "audiolibro"
        | "livre audio"
        | "buku audio"
        | "오디오북"
        | "luisterboek"
        | "аудиокнига"
        | "ljudbok"
        | "sesli kitap"
        | "аудіокнига" => ContentKind::Audiobook,
        "artist"
        | "فنان"
        | "interpret"
        | "artista"
        | "artiste"
        | "artis"
        | "아티스트"
        | "artiest"
        | "artysta"
        | "исполнитель"
        | "sanatçı"
        | "виконавець" => ContentKind::Artist,
        "profile"
        | "الملف الشخصي"
        | "profil"
        | "perfil"
        | "profilo"
        | "프로필"
        | "profiel"
        | "профиль"
        | "профіль" => ContentKind::Profile,
        "playlist"
        | "قائمة تشغيل"
        | "lista de reproducción"
        | "daftar putar"
        | "재생목록"
        | "afspeellijst"
        | "playlista"
        | "lista de reprodução"
        | "плейлист"
        | "spellista"
        | "çalma listesi" => ContentKind::Playlist,
        "podcast" | "بودكاست" | "팟캐스트" | "подкаст" | "podd" => {
            ContentKind::PodcastShow
        }
        "episode" | "podcast episode" | "حلقة" | "folge" | "episodio" | "épisode" | "에피소드"
        | "aflevering" | "odcinek" | "episódio" | "выпуск" | "avsnitt" | "bölüm" | "епізод" => {
            ContentKind::PodcastEpisode
        }
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn parse_rows(rows: [Value; 2]) -> SearchResults {
        parse_search(&json!({
            "contents": {"sectionListRenderer": {"contents": rows}}
        }))
    }

    #[test]
    fn dedupes_prefixed_and_raw_playlist_ids() {
        let playlist_row = |id, title| {
            json!({
                "musicTwoRowItemRenderer": {
                    "title": {"runs": [{"text": title}]},
                    "navigationEndpoint": {"browseEndpoint": {
                        "browseId": id,
                        "browseEndpointContextSupportedConfigs": {
                            "browseEndpointContextMusicConfig": {"pageType": "MUSIC_PAGE_TYPE_PLAYLIST"}
                        }
                    }}
                }
            })
        };

        for (first_id, second_id) in [
            ("VLtest-playlist", "test-playlist"),
            ("test-playlist", "VLtest-playlist"),
        ] {
            let results = parse_rows([
                playlist_row(first_id, "First playlist"),
                playlist_row(second_id, "Second playlist"),
            ]);

            assert!(
                matches!(results.items.as_slice(), [SearchItem::Playlist(card)]
                if card.browse_id == first_id && card.title == "First playlist")
            );
        }
    }

    #[test]
    fn dedupes_songs_and_videos_with_the_same_video_id() {
        let playable_row = |kind, title| {
            json!({
                "musicTwoRowItemRenderer": {
                    "title": {"runs": [{"text": title}]},
                    "subtitle": {"runs": [{"text": kind}]},
                    "navigationEndpoint": {"watchEndpoint": {"videoId": "test-video0"}}
                }
            })
        };
        let song = playable_row("Song", "Song result");
        let video = playable_row("Video", "Video result");

        let song_first = parse_rows([song.clone(), video.clone()]);
        assert!(
            matches!(song_first.items.as_slice(), [SearchItem::Song(item)]
            if item.video_id == "test-video0" && item.title == "Song result")
        );

        let video_first = parse_rows([video, song]);
        assert!(
            matches!(video_first.items.as_slice(), [SearchItem::Video(item)]
            if item.video_id == "test-video0" && item.title == "Video result")
        );
    }
}
