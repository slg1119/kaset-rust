use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArtistRef {
    pub name: String,
    pub browse_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RadioQueue {
    pub songs: Vec<Song>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowseCard {
    pub browse_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowsePage {
    pub title: String,
    pub subtitle: Option<String>,
    pub second_subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
    pub songs: Vec<Song>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShelfItem {
    Song(Song),
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<ShelfItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomePage {
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}

/// One search result, in server order. Songs, videos, and episodes carry a
/// video id and can be queued; the rest open detail pages (milestone 5).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchItem {
    Song(Song),
    Video(Song),
    /// A podcast episode; `artists` holds the show when the result links it.
    Episode(Song),
    /// Albums, singles, EPs, and audiobooks.
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
    /// A user channel (`MUSIC_PAGE_TYPE_USER_CHANNEL`).
    Profile(BrowseCard),
    Podcast(BrowseCard),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResults {
    pub items: Vec<SearchItem>,
    /// Token for the next page; not requested yet.
    pub continuation: Option<String>,
}

/// The signed-in YouTube Music account, for display only.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Account {
    pub name: String,
    /// Channel handle such as `@name`, when the account has one.
    pub handle: Option<String>,
    pub photo_url: Option<String>,
}

#[cfg(all(test, feature = "specta"))]
mod specta_tests {
    use super::*;

    fn assert_type<T: specta::Type>() {}

    #[test]
    fn models_implement_specta_type() {
        assert_type::<ArtistRef>();
        assert_type::<Song>();
        assert_type::<RadioQueue>();
        assert_type::<BrowseCard>();
        assert_type::<BrowsePage>();
        assert_type::<ShelfItem>();
        assert_type::<HomeSection>();
        assert_type::<HomePage>();
        assert_type::<SearchItem>();
        assert_type::<SearchResults>();
        assert_type::<Account>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_round_trips_through_json() {
        let song = Song {
            video_id: "test-video1".into(),
            title: "Test song".into(),
            artists: vec![ArtistRef {
                name: "Test artist".into(),
                browse_id: Some("test-browse".into()),
            }],
            thumbnail_url: None,
        };
        let json = serde_json::to_string(&song).unwrap();
        assert_eq!(serde_json::from_str::<Song>(&json).unwrap(), song);
    }
}
