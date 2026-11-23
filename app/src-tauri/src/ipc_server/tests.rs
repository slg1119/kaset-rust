use std::sync::Mutex;

use kaset_ipc::{Error, PlaybackStatus, RepeatMode, Track};
use ytm_core::models::ArtistRef;

use super::*;

struct FakePlayer {
    signed_in: bool,
    engine: Mutex<Player>,
    page_id: Mutex<Option<String>>,
}

impl FakePlayer {
    fn new(signed_in: bool) -> Self {
        Self {
            signed_in,
            engine: Mutex::new(Player::new(1)),
            page_id: Mutex::new(None),
        }
    }

    fn check_auth(&self) -> Result<(), AppError> {
        if self.signed_in {
            Ok(())
        } else {
            Err(AppError::not_signed_in())
        }
    }
}

impl Playback for FakePlayer {
    async fn snapshot(&self) -> Result<(PlayerState, QueueState), AppError> {
        self.check_auth()?;
        let player = self.engine.lock().unwrap();
        Ok((player.state(), player.queue_state()))
    }

    async fn change(&self, action: Action) -> Result<(), AppError> {
        self.check_auth()?;
        action.apply(&mut self.engine.lock().unwrap(), Instant::now())?;
        Ok(())
    }

    async fn play_page(&self, browse_id: String) -> Result<(), AppError> {
        self.check_auth()?;
        *self.page_id.lock().unwrap() = Some(browse_id);
        self.change(Action::Replace(vec![
            Song {
                album: None,
                is_podcast_episode: None,
                video_id: "test-vid001".into(),
                title: "Test song".into(),
                artists: vec![ArtistRef {
                    name: "Test artist".into(),
                    browse_id: None,
                }],
                thumbnail_url: None,
            },
            Song {
                album: None,
                is_podcast_episode: None,
                video_id: "test-vid002".into(),
                title: "Second song".into(),
                artists: vec![],
                thumbnail_url: None,
            },
        ]))
        .await
    }
}

#[tokio::test]
async fn ipc_dispatch_rejects_signed_out_player() {
    let player = FakePlayer::new(false);
    for command in [
        Command::Status {},
        Command::Queue {},
        Command::Play {},
        Command::PlayVideo {
            video_id: "abc-DEF_123".into(),
        },
        Command::PlayPage {
            browse_id: "VL-test".into(),
        },
    ] {
        assert_eq!(
            dispatch(&player, command).await,
            Response::Error {
                error: Error::NotSignedIn
            }
        );
    }
    assert!(player.page_id.lock().unwrap().is_none());
}

#[tokio::test]
async fn ipc_dispatch_controls_the_player_and_returns_snapshots() {
    let player = FakePlayer::new(true);
    assert_eq!(
        dispatch(
            &player,
            Command::PlayVideo {
                video_id: "abc-DEF_123".into()
            }
        )
        .await,
        Response::Ok
    );
    {
        let engine = player.engine.lock().unwrap();
        assert_eq!(engine.queue_state().songs.len(), 1);
        assert!(engine.needs_more().is_some()); // Normal service autoplay will seed radio.
    }
    assert_eq!(
        dispatch(
            &player,
            Command::PlayPage {
                browse_id: "VL-test".into()
            }
        )
        .await,
        Response::Ok
    );
    assert_eq!(player.page_id.lock().unwrap().as_deref(), Some("VL-test"));

    for (command, expected) in [
        (Command::Pause {}, PlaybackStatus::Paused),
        (Command::Play {}, PlaybackStatus::Playing),
        (Command::PlayPause {}, PlaybackStatus::Paused),
    ] {
        assert_eq!(dispatch(&player, command).await, Response::Ok);
        let Response::Status { status } = dispatch(&player, Command::Status {}).await else {
            panic!("expected a status response");
        };
        assert_eq!(status.status, expected);
        assert_eq!(
            status.track,
            Some(Track {
                title: "Test song".into(),
                artists: vec!["Test artist".into()],
                video_id: "test-vid001".into(),
            })
        );
    }

    for command in [
        Command::Next {},
        Command::Previous {},
        Command::Seek { seconds: 12.5 },
        Command::Shuffle { on: true },
        Command::Repeat {
            mode: RepeatMode::One,
        },
    ] {
        assert_eq!(dispatch(&player, command).await, Response::Ok);
    }
    let Response::Status { status } = dispatch(&player, Command::Status {}).await else {
        panic!("expected a status response");
    };
    assert_eq!(status.position, 12.5);
    assert_eq!(status.repeat, RepeatMode::One);
    assert!(status.shuffle);
    let Response::Queue { queue } = dispatch(&player, Command::Queue {}).await else {
        panic!("expected a queue response");
    };
    assert_eq!(queue.current, Some(0));
    assert_eq!(queue.songs.len(), 2);
}
