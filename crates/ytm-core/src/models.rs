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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<PlaylistActions>,
    /// Playlist item IDs in the same order as songs, including duplicate videos.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_video_ids: Option<Vec<Option<String>>>,
    pub title: String,
    pub subtitle: Option<String>,
    pub second_subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
    pub songs: Vec<Song>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "UPPERCASE")]
pub enum LikeStatus {
    Like,
    Dislike,
    Indifferent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "UPPERCASE")]
pub enum PlaylistPrivacy {
    Public,
    Unlisted,
    Private,
}

/// Full metadata stays on the Rust side. Feedback tokens must never cross IPC.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SongMetadata {
    pub like_status: Option<LikeStatus>,
    pub in_library: bool,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub add_token: Option<crate::secret::Secret>,
    #[serde(skip)]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub remove_token: Option<crate::secret::Secret>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AddToPlaylistMenu {
    pub title: Option<String>,
    pub options: Vec<AddToPlaylistOption>,
    pub can_create_playlist: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AddToPlaylistOption {
    pub playlist_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
    pub is_selected: bool,
    pub privacy_status: Option<PlaylistPrivacy>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistActions {
    pub can_delete: bool,
    pub saved: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Episode {
    pub video_id: String,
    pub title: String,
    pub description: Option<String>,
    pub date: Option<String>,
    pub duration: Option<String>,
    pub thumbnail_url: Option<String>,
    /// Server playback progress as a fraction from 0 to 1.
    pub progress: Option<f64>,
    pub show_title: Option<String>,
    pub show_browse_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PodcastShow {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub episodes: Vec<Episode>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PodcastEpisodesPage {
    pub episodes: Vec<Episode>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowsePageContinuation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_video_ids: Option<Vec<Option<String>>>,
    pub songs: Vec<Song>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LibraryPage {
    pub sections: Vec<HomeSection>,
    pub continuations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShelfItem {
    Song(Song),
    Podcast(BrowseCard),
    Episode(Episode),
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
    Category {
        title: String,
        browse_id: String,
        params: Option<String>,
        color: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<ShelfItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub more: Option<BrowseEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowseEndpoint {
    pub browse_id: String,
    pub params: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArtistPage {
    pub name: String,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub subscribers: Option<String>,
    pub channel_id: Option<String>,
    pub subscribed: Option<bool>,
    pub radio_playlist_id: Option<String>,
    pub radio_video_id: Option<String>,
    pub radio_params: Option<String>,
    pub shuffle_playlist_id: Option<String>,
    pub shuffle_video_id: Option<String>,
    pub shuffle_params: Option<String>,
    /// Index of the first playable music shelf, distinct from video carousels.
    pub top_songs_section: Option<u32>,
    pub sections: Vec<HomeSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HomePage {
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Chip {
    pub title: String,
    pub params: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FeedPage {
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
    pub chips: Vec<Chip>,
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

/// A server-side YouTube Music search filter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SearchFilter {
    Songs,
    Videos,
    Albums,
    Artists,
    Profiles,
    Playlists,
    FeaturedPlaylists,
    CommunityPlaylists,
    Podcasts,
    Episodes,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SearchResults {
    pub items: Vec<SearchItem>,
    /// Token for the next page.
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

/// Untimed YouTube Music lyrics and optional attribution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Lyrics {
    pub text: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LyricLine {
    pub start_ms: u32,
    pub end_ms: Option<u32>,
    pub text: String,
}

/// Best available lyrics for a track. A missing text and line list means unavailable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LyricsResult {
    pub lines: Option<Vec<LyricLine>>,
    pub text: Option<String>,
    pub source: Option<String>,
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
        assert_type::<LikeStatus>();
        assert_type::<PlaylistPrivacy>();
        assert_type::<SongMetadata>();
        assert_type::<AddToPlaylistMenu>();
        assert_type::<AddToPlaylistOption>();
        assert_type::<PlaylistActions>();
        assert_type::<Episode>();
        assert_type::<PodcastShow>();
        assert_type::<PodcastEpisodesPage>();
        assert_type::<BrowsePageContinuation>();
        assert_type::<LibraryPage>();
        assert_type::<ShelfItem>();
        assert_type::<HomeSection>();
        assert_type::<BrowseEndpoint>();
        assert_type::<ArtistPage>();
        assert_type::<HomePage>();
        assert_type::<Chip>();
        assert_type::<FeedPage>();
        assert_type::<SearchItem>();
        assert_type::<SearchFilter>();
        assert_type::<SearchResults>();
        assert_type::<Account>();
        assert_type::<Lyrics>();
        assert_type::<LyricLine>();
        assert_type::<LyricsResult>();
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
