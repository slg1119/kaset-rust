use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtistRef {
    pub name: String,
    pub browse_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowseCard {
    pub browse_id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShelfItem {
    Song(Song),
    Album(BrowseCard),
    Playlist(BrowseCard),
    Artist(BrowseCard),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HomeSection {
    pub title: String,
    pub items: Vec<ShelfItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HomePage {
    pub sections: Vec<HomeSection>,
    pub continuation: Option<String>,
}
