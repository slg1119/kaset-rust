use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::json::{at, str_at};
use crate::models::ArtistRef;

const BROWSE_ENDPOINT: &[&str] = &["navigationEndpoint", "browseEndpoint"];

pub(super) fn page_type(browse: &Value) -> Option<&str> {
    str_at(
        browse,
        &[
            "browseEndpointContextSupportedConfigs",
            "browseEndpointContextMusicConfig",
            "pageType",
        ],
    )
}

pub(super) fn is_artist_page_type(page_type: Option<&str>) -> bool {
    matches!(
        page_type,
        Some(
            "MUSIC_PAGE_TYPE_ARTIST"
                | "MUSIC_PAGE_TYPE_USER_CHANNEL"
                | "MUSIC_PAGE_TYPE_LIBRARY_ARTIST"
        )
    )
}

pub(super) fn is_navigable_artist_id(id: &str) -> bool {
    id.starts_with("UC") || id.starts_with("MPLAUC")
}

/// Bullet-delimited subtitle fields with linked artists take priority.
pub(super) fn artists_from_runs(text: Option<&Value>) -> Vec<ArtistRef> {
    let runs = artist_runs(text);
    let mut linked = Vec::new();
    let mut fallback = None;

    for field in runs.split(|run| matches!(run_text(run), Some("•" | "·"))) {
        let artists: Vec<_> = field
            .iter()
            .filter_map(|run| {
                let name = artist_name(run)?;
                if let Some(browse) = at(run, BROWSE_ENDPOINT) {
                    let id = browse.get("browseId")?.as_str()?;
                    return (is_navigable_artist_id(id) || is_artist_page_type(page_type(browse)))
                        .then(|| ArtistRef {
                            name: name.into(),
                            browse_id: Some(id.into()),
                        });
                }
                unlinked_artist(run, name)
            })
            .collect();

        if artists.iter().any(|artist| artist.browse_id.is_some()) {
            linked.extend(artists);
        } else if fallback.is_none() && !artists.is_empty() {
            fallback = Some(artists);
        }
    }

    if linked.is_empty() {
        fallback.unwrap_or_default()
    } else {
        linked
    }
}

/// Flex columns use navigable links first and fall back to one plain artist name.
pub(super) fn artists_from_flex_column(text: Option<&Value>) -> Vec<ArtistRef> {
    let mut linked = Vec::new();
    let mut fallback = None;

    for run in artist_runs(text) {
        let Some(name) = artist_name(run) else {
            continue;
        };
        let id = at(run, BROWSE_ENDPOINT).and_then(|browse| str_at(browse, &["browseId"]));
        if let Some(id) = id.filter(|id| is_navigable_artist_id(id)) {
            linked.push(ArtistRef {
                name: name.into(),
                browse_id: Some(id.into()),
            });
        } else if fallback.is_none() {
            fallback = unlinked_artist(run, name);
        }
    }

    if linked.is_empty() {
        fallback.into_iter().collect()
    } else {
        linked
    }
}

fn artist_runs(text: Option<&Value>) -> &[Value] {
    text.and_then(|text| text.get("runs"))
        .and_then(Value::as_array)
        .filter(|runs| runs.iter().all(Value::is_object))
        .map_or(&[], Vec::as_slice)
}

fn run_text(run: &Value) -> Option<&str> {
    Some(run.get("text")?.as_str()?.trim())
}

fn artist_name(run: &Value) -> Option<&str> {
    run_text(run).filter(|name| !matches!(*name, "" | "•" | "·" | "&" | ","))
}

fn unlinked_artist(run: &Value, name: &str) -> Option<ArtistRef> {
    (run.get("navigationEndpoint").is_none() && !is_metadata_text(name)).then(|| ArtistRef {
        name: name.into(),
        browse_id: None,
    })
}

fn is_metadata_text(text: &str) -> bool {
    let lowercased = text.to_lowercase();
    LOCALIZED_CONTENT_TYPES.contains(&lowercased.as_str())
        || is_english_engagement_count(&lowercased)
        || is_clock_duration(text)
        || is_natural_language_duration(&lowercased)
        || is_song_count(&lowercased)
        || (text.len() == 4
            && text
                .parse::<u16>()
                .is_ok_and(|year| (1900..=2100).contains(&year)))
}

fn is_english_engagement_count(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let (Some(count), Some(unit), None) = (words.next(), words.next(), words.next()) else {
        return false;
    };
    matches!(
        unit,
        "episode" | "episodes" | "play" | "plays" | "subscriber" | "subscribers" | "view" | "views"
    ) && (count == "no" || count.chars().all(|ch| "0123456789.,+kmbt".contains(ch)))
}

pub(super) fn is_clock_duration(text: &str) -> bool {
    if !text.contains(':') {
        return false;
    }
    let mut count = 0;
    for component in text.split(':').filter(|part| !part.is_empty()) {
        if component.parse::<i64>().is_err() {
            return false;
        }
        count += 1;
    }
    matches!(count, 2 | 3)
}

fn is_natural_language_duration(text: &str) -> bool {
    ["second", "minute", "hour"]
        .iter()
        .any(|unit| text.contains(unit))
        && text
            .chars()
            .all(|ch| ch.is_numeric() || ch.is_whitespace() || ",secondminuteshour".contains(ch))
}

static EDGE_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\p{P}+|\p{P}+$").expect("static regex is valid"));

fn is_song_count(text: &str) -> bool {
    if !text.contains("song") && !text.contains("track") {
        return false;
    }
    let words = text.split_whitespace();
    words.clone().zip(words.skip(1)).any(|(count, unit)| {
        let unit = EDGE_PUNCTUATION.replace_all(unit, "");
        matches!(unit.as_ref(), "song" | "songs" | "track" | "tracks")
            && count.chars().all(|ch| ch.is_ascii_digit() || ch == ',')
            && count.replace(',', "").parse::<i64>().is_ok()
    })
}

// Kept in sync with ParsingHelpers+Metadata.swift.
#[rustfmt::skip]
const LOCALIZED_CONTENT_TYPES: &[&str] = &[
    "album", "single", "ep", "ألبوم", "أغنية منفردة", "álbum", "sencillo", "singel", "singolo", "싱글",
    "singiel", "сингл", "tekli", "앨범", "альбом", "albüm",
    "artist", "فنان", "interpret", "artista", "artiste", "artis", "아티스트", "artiest", "artysta",
    "исполнитель", "sanatçı", "виконавець",
    "audiobook", "كتاب صوتي", "hörbuch", "audiolibro", "livre audio", "buku audio", "오디오북",
    "luisterboek", "аудиокнига", "ljudbok", "sesli kitap", "аудіокнига",
    "episode", "podcast episode", "حلقة", "folge", "episodio", "épisode", "에피소드", "aflevering",
    "odcinek", "episódio", "выпуск", "avsnitt", "bölüm", "епізод",
    "playlist", "قائمة تشغيل", "lista de reproducción", "daftar putar", "재생목록", "afspeellijst",
    "playlista", "lista de reprodução", "плейлист", "spellista", "çalma listesi",
    "podcast", "بودكاست", "팟캐스트", "подкаст", "podd",
    "profile", "الملف الشخصي", "profil", "perfil", "profilo", "프로필", "profiel", "профиль", "профіль",
    "song", "أغنية", "titel", "canción", "morceau", "lagu", "brano", "노래", "nummer", "utwór",
    "música", "трек", "låt", "şarkı", "пісня",
    "video", "فيديو", "vídeo", "vidéo", "동영상", "wideo", "видео", "відео",
];
