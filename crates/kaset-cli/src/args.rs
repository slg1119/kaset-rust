use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use ytm_core::models::SearchItem;

#[derive(Parser)]
#[command(name = "kaset", version, about)]
pub(crate) struct Cli {
    /// Print the parsed response as JSON.
    #[arg(long, global = true)]
    pub(crate) json: bool,
    /// Read a session file from a debug build of the Kaset app.
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) session_file: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Search YouTube Music.
    Search {
        #[arg(value_parser = search_query)]
        query: String,
        /// Keep only this result type from the first search page.
        #[arg(long, value_enum)]
        filter: Option<SearchFilter>,
        #[arg(long)]
        limit: Option<usize>,
    },
    #[command(flatten)]
    Feed(Feed),
    /// List the tracks on an album or playlist page.
    Page {
        #[arg(value_name = "BROWSE_ID", value_parser = browse_id)]
        browse_id: String,
    },
    /// List the tracks in a radio mix seeded by a video.
    Radio {
        #[arg(value_name = "VIDEO_ID", value_parser = video_id)]
        video_id: String,
    },
    /// Show the signed-in YouTube Music account.
    Whoami,
}

#[derive(Subcommand, Clone, Copy)]
pub(crate) enum Feed {
    /// Browse the Home feed.
    Home,
    /// Browse the Explore feed.
    Explore,
    /// Browse the Charts feed.
    Charts,
    /// Browse moods and genres.
    Moods,
    /// Browse new releases.
    New,
    /// Browse your listening history.
    History,
}

impl Feed {
    pub(crate) fn browse_id(self) -> &'static str {
        match self {
            Self::Home => "FEmusic_home",
            Self::Explore => "FEmusic_explore",
            Self::Charts => "FEmusic_charts",
            Self::Moods => "FEmusic_moods_and_genres",
            Self::New => "FEmusic_new_releases",
            Self::History => "FEmusic_history",
        }
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum SearchFilter {
    Songs,
    Albums,
    Artists,
    Playlists,
    Videos,
}

impl SearchFilter {
    pub(crate) fn matches(self, item: &SearchItem) -> bool {
        matches!(
            (self, item),
            (Self::Songs, SearchItem::Song(_))
                | (Self::Albums, SearchItem::Album(_))
                | (Self::Artists, SearchItem::Artist(_))
                | (Self::Playlists, SearchItem::Playlist(_))
                | (Self::Videos, SearchItem::Video(_))
        )
    }
}

fn search_query(value: &str) -> Result<String, &'static str> {
    let query = value.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Err("expected a query of 1 to 200 characters");
    }
    Ok(query.into())
}

fn browse_id(value: &str) -> Result<String, &'static str> {
    if !(1..=256).contains(&value.len()) || !id_chars(value) {
        return Err(
            "expected a browse ID of 1 to 256 ASCII letters, digits, underscores or hyphens",
        );
    }
    Ok(value.into())
}

fn video_id(value: &str) -> Result<String, &'static str> {
    if value.len() != 11 || !id_chars(value) {
        return Err("expected a video ID of 11 ASCII letters, digits, underscores or hyphens");
    }
    Ok(value.into())
}

fn id_chars(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
