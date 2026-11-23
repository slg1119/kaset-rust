//! Same-user CLI commands, routed through the desktop session and player.
use std::time::Instant;

use kaset_ipc::{Command, Error, Queue, Response, Status, Track};
use tauri::{AppHandle, Manager};
use ytm_core::models::Song;

use crate::commands::load_browse_page;
use crate::error::{AppError, ErrorKind};
use crate::player::commands::replace_queue;
use crate::player::engine::{Outcome, PlaybackStatus, Player, PlayerState, QueueState};
use crate::player::queue::{MAX_QUEUE_LEN, RepeatMode};
use crate::player::video_id::VideoId;
use crate::session::{AuthStatus, Session};
use crate::state::AppState;

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = kaset_ipc::serve(move |command| {
            let app = app.clone();
            async move { dispatch(&DesktopPlayer(app), command).await }
        })
        .await
        {
            tracing::warn!(%error, "local playback IPC listener stopped");
        }
    });
}

// The backend owns the player lock, signed-in checks and any API I/O.
trait Playback {
    async fn snapshot(&self) -> Result<(PlayerState, QueueState), AppError>;
    async fn change(&self, action: Action) -> Result<(), AppError>;
    async fn play_page(&self, browse_id: String) -> Result<(), AppError>;
}

enum Action {
    Toggle,
    Play,
    Pause,
    Next,
    Previous,
    Seek(f64),
    Replace(Vec<Song>),
    Shuffle(bool),
    Repeat(RepeatMode),
}

impl Action {
    fn apply(self, player: &mut Player, now: Instant) -> Result<Outcome, AppError> {
        Ok(match self {
            Self::Toggle => player.toggle(now),
            Self::Play => player.play(now),
            Self::Pause => player.pause(now),
            Self::Next => player.next(now),
            Self::Previous => player.previous(now),
            Self::Seek(seconds) => player.seek(seconds, now),
            Self::Replace(songs) => return replace_queue(player, songs, 0, now),
            Self::Shuffle(on) => player.set_shuffle(on, now),
            Self::Repeat(mode) => player.set_repeat(mode, now),
        })
    }
}

struct DesktopPlayer(AppHandle);

fn check_session(session: &Session, epoch: u64) -> Result<(), AppError> {
    if session.epoch() != epoch {
        return Err(AppError::not_signed_in());
    }
    match session.status() {
        AuthStatus::SignedIn => Ok(()),
        AuthStatus::SignedOut => Err(AppError::not_signed_in()),
        AuthStatus::Expired => Err(ytm_core::YtmError::AuthExpired.into()),
    }
}

impl Playback for DesktopPlayer {
    async fn snapshot(&self) -> Result<(PlayerState, QueueState), AppError> {
        let state = self.0.state::<AppState>();
        let epoch = state.session.epoch();
        check_session(&state.session, epoch)?;
        let snapshot = state.player.snapshot().await;
        check_session(&state.session, epoch)?;
        Ok(snapshot)
    }

    async fn change(&self, action: Action) -> Result<(), AppError> {
        let state = self.0.state::<AppState>();
        let epoch = state.session.epoch();
        state
            .player
            .update(&self.0, |player, now| {
                check_session(&state.session, epoch)?;
                action.apply(player, now)
            })
            .await
    }

    async fn play_page(&self, browse_id: String) -> Result<(), AppError> {
        let state = self.0.state::<AppState>();
        let epoch = state.session.epoch();
        check_session(&state.session, epoch)?;
        let generation = state.player.queue_generation().await;
        let mut page = load_browse_page(&self.0, &state, browse_id).await?;
        page.songs
            .retain(|song| VideoId::parse(&song.video_id).is_some());
        page.songs.truncate(MAX_QUEUE_LEN);
        state
            .player
            .update(&self.0, |player, now| {
                check_session(&state.session, epoch)?;
                // A slow browse must not overwrite a newer queue.
                if player.queue_position().0 != generation {
                    return Err(AppError::internal(
                        "The queue changed while the page loaded.",
                    ));
                }
                Action::Replace(page.songs).apply(player, now)
            })
            .await
    }
}

async fn dispatch(player: &impl Playback, command: Command) -> Response {
    match execute(player, command).await {
        Ok(response) => response,
        Err(error) => Response::Error {
            error: match error.kind {
                ErrorKind::NotSignedIn => Error::NotSignedIn,
                ErrorKind::AuthExpired => Error::AuthExpired,
                _ => Error::PlaybackFailed,
            },
        },
    }
}

async fn execute(player: &impl Playback, command: Command) -> Result<Response, AppError> {
    let action = match command {
        Command::Status {} => {
            let (state, _) = player.snapshot().await?;
            return Ok(Response::Status {
                status: Status {
                    track: state.track.map(track),
                    position: state.position,
                    duration: state.duration,
                    status: match state.status {
                        PlaybackStatus::Idle => kaset_ipc::PlaybackStatus::Idle,
                        PlaybackStatus::Loading => kaset_ipc::PlaybackStatus::Loading,
                        PlaybackStatus::Playing => kaset_ipc::PlaybackStatus::Playing,
                        PlaybackStatus::Paused => kaset_ipc::PlaybackStatus::Paused,
                        PlaybackStatus::Ended => kaset_ipc::PlaybackStatus::Ended,
                    },
                    repeat: match state.repeat {
                        RepeatMode::Off => kaset_ipc::RepeatMode::Off,
                        RepeatMode::All => kaset_ipc::RepeatMode::All,
                        RepeatMode::One => kaset_ipc::RepeatMode::One,
                    },
                    shuffle: state.shuffle,
                },
            });
        }
        Command::Queue {} => {
            let (_, queue) = player.snapshot().await?;
            return Ok(Response::Queue {
                queue: Queue {
                    songs: queue.songs.into_iter().map(track).collect(),
                    current: queue.current,
                },
            });
        }
        Command::PlayPage { browse_id } => {
            player.play_page(browse_id).await?;
            return Ok(Response::Ok);
        }
        Command::PlayVideo { video_id } => Action::Replace(vec![Song {
            is_podcast_episode: None,
            title: video_id.clone(),
            video_id,
            artists: vec![],
            thumbnail_url: None,
        }]),
        Command::PlayPause {} => Action::Toggle,
        Command::Play {} => Action::Play,
        Command::Pause {} => Action::Pause,
        Command::Next {} => Action::Next,
        Command::Previous {} => Action::Previous,
        Command::Seek { seconds } => Action::Seek(seconds),
        Command::Shuffle { on } => Action::Shuffle(on),
        Command::Repeat { mode } => Action::Repeat(match mode {
            kaset_ipc::RepeatMode::Off => RepeatMode::Off,
            kaset_ipc::RepeatMode::All => RepeatMode::All,
            kaset_ipc::RepeatMode::One => RepeatMode::One,
        }),
    };
    player.change(action).await?;
    Ok(Response::Ok)
}

fn track(song: Song) -> Track {
    Track {
        title: song.title,
        artists: song.artists.into_iter().map(|artist| artist.name).collect(),
        video_id: song.video_id,
    }
}

#[cfg(test)]
mod tests;
