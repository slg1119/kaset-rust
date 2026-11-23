#[cfg(debug_assertions)]
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use kaset_ipc::Command as PlaybackRequest;
use ytm_core::models::SearchItem;

#[derive(Parser)]
#[command(name = "kaset", version, about)]
pub(crate) struct Cli {
    /// Print the parsed response as JSON.
    #[arg(long, global = true)]
    pub(crate) json: bool,
    /// Read a session file from a debug build of the Kaset app.
    #[cfg(debug_assertions)]
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) session_file: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(flatten)]
    Api(ApiCommand),
    #[command(flatten)]
    Playback(PlaybackCommand),
}

#[derive(Subcommand)]
pub(crate) enum ApiCommand {
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

#[derive(Subcommand)]
pub(crate) enum PlaybackCommand {
    /// Show what is playing in Kaset.
    Now,
    /// Resume playback, play one video, or play an album or playlist.
    Play {
        #[arg(value_name = "VIDEO_ID", value_parser = video_id, conflicts_with = "page")]
        video_id: Option<String>,
        #[arg(long, value_name = "BROWSE_ID", value_parser = playback_browse_id)]
        page: Option<String>,
    },
    /// Pause playback.
    Pause,
    /// Toggle between playing and paused.
    Toggle,
    /// Skip to the next track.
    Next,
    /// Restart the current track or go to the previous track.
    Prev,
    /// Seek using seconds or minutes:seconds.
    Seek {
        #[arg(value_name = "TIME", value_parser = seek_seconds)]
        seconds: f64,
    },
    /// List the playback queue and mark the current track.
    Queue,
    /// Turn shuffle on or off.
    Shuffle {
        #[arg(value_enum)]
        mode: ShuffleMode,
    },
    /// Set repeat mode.
    Repeat {
        #[arg(value_enum)]
        mode: RepeatMode,
    },
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum ShuffleMode {
    On,
    Off,
}

#[derive(ValueEnum, Clone, Copy)]
pub(crate) enum RepeatMode {
    Off,
    All,
    One,
}

impl From<PlaybackCommand> for PlaybackRequest {
    fn from(command: PlaybackCommand) -> Self {
        match command {
            PlaybackCommand::Now => Self::Status {},
            PlaybackCommand::Play {
                page: Some(browse_id),
                ..
            } => Self::PlayPage { browse_id },
            PlaybackCommand::Play {
                video_id: Some(video_id),
                ..
            } => Self::PlayVideo { video_id },
            PlaybackCommand::Play { .. } => Self::Play {},
            PlaybackCommand::Pause => Self::Pause {},
            PlaybackCommand::Toggle => Self::PlayPause {},
            PlaybackCommand::Next => Self::Next {},
            PlaybackCommand::Prev => Self::Previous {},
            PlaybackCommand::Seek { seconds } => Self::Seek { seconds },
            PlaybackCommand::Queue => Self::Queue {},
            PlaybackCommand::Shuffle { mode } => Self::Shuffle {
                on: matches!(mode, ShuffleMode::On),
            },
            PlaybackCommand::Repeat { mode } => Self::Repeat {
                mode: match mode {
                    RepeatMode::Off => kaset_ipc::RepeatMode::Off,
                    RepeatMode::All => kaset_ipc::RepeatMode::All,
                    RepeatMode::One => kaset_ipc::RepeatMode::One,
                },
            },
        }
    }
}

fn playback_browse_id(value: &str) -> Result<String, &'static str> {
    if !kaset_ipc::valid_browse_id(value) {
        return Err(
            "expected a browse ID of 2 to 128 ASCII letters, digits, underscores or hyphens",
        );
    }
    Ok(value.into())
}

fn seek_seconds(value: &str) -> Result<f64, &'static str> {
    const INVALID: &str = "expected non-negative seconds or minutes:seconds";
    let seconds = if let Some((minutes, seconds)) = value.split_once(':') {
        let minutes = minutes.parse::<u64>().map_err(|_| INVALID)?;
        let seconds = seconds.parse::<f64>().map_err(|_| INVALID)?;
        if !(0.0..60.0).contains(&seconds) {
            return Err(INVALID);
        }
        minutes as f64 * 60.0 + seconds
    } else {
        value.parse().map_err(|_| INVALID)?
    };
    if !kaset_ipc::valid_seek(seconds) {
        return Err(INVALID);
    }
    Ok(seconds)
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
    if !kaset_ipc::valid_video_id(value) {
        return Err("expected a video ID of 11 ASCII letters, digits, underscores or hyphens");
    }
    Ok(value.into())
}

fn id_chars(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
