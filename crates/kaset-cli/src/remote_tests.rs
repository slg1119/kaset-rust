use clap::Parser;
use kaset_ipc::{
    ClientError, Command, Error, PlaybackStatus, Queue, RepeatMode, Response, Status, Track,
};
use serde_json::{Value, json};

use crate::{Cli, CliError, ExitCode, args, output::write_playback};

#[test]
fn parses_playback_commands_and_validates_playback_arguments() {
    for (args, expected) in [
        (vec!["now"], Command::Status {}),
        (vec!["play"], Command::Play {}),
        (
            vec!["play", "abc-DEF_123"],
            Command::PlayVideo {
                video_id: "abc-DEF_123".into(),
            },
        ),
        (
            vec!["play", "--page", "VL-test"],
            Command::PlayPage {
                browse_id: "VL-test".into(),
            },
        ),
        (vec!["pause"], Command::Pause {}),
        (vec!["toggle"], Command::PlayPause {}),
        (vec!["next"], Command::Next {}),
        (vec!["prev"], Command::Previous {}),
        (vec!["seek", "02:03.5"], Command::Seek { seconds: 123.5 }),
        (vec!["seek", "23.5"], Command::Seek { seconds: 23.5 }),
        (vec!["queue"], Command::Queue {}),
        (vec!["shuffle", "on"], Command::Shuffle { on: true }),
        (vec!["shuffle", "off"], Command::Shuffle { on: false }),
        (
            vec!["repeat", "off"],
            Command::Repeat {
                mode: RepeatMode::Off,
            },
        ),
        (
            vec!["repeat", "all"],
            Command::Repeat {
                mode: RepeatMode::All,
            },
        ),
        (
            vec!["repeat", "one"],
            Command::Repeat {
                mode: RepeatMode::One,
            },
        ),
    ] {
        let cli = Cli::try_parse_from(["kaset", "--json"].into_iter().chain(args)).unwrap();
        assert!(cli.json);
        let args::Command::Playback(command) = cli.command else {
            panic!("expected a playback command");
        };
        assert_eq!(Command::from(command), expected);
    }
    for args in [
        vec!["play", "bad/id"],
        vec!["play", "--page", "x"],
        vec!["play", "abc-DEF_123", "--page", "VL-test"],
        vec!["seek", "-1"],
        vec!["seek", "1:60"],
        vec!["seek", "1:2:3"],
        vec!["seek", "NaN"],
        vec!["shuffle", "yes"],
        vec!["repeat", "forever"],
    ] {
        assert!(Cli::try_parse_from(std::iter::once("kaset").chain(args)).is_err());
    }
}

#[test]
fn formats_status_and_queue_as_text_and_json() {
    let track = Track {
        title: "Test\u{1b}song".into(),
        artists: vec!["First artist".into(), "Second artist".into()],
        video_id: "test-vid001".into(),
    };
    let status = Response::Status {
        status: Status {
            track: Some(track.clone()),
            position: 123.5,
            duration: 360.0,
            status: PlaybackStatus::Playing,
            repeat: RepeatMode::All,
            shuffle: true,
        },
    };
    let queue = Response::Queue {
        queue: Queue {
            songs: vec![track],
            current: Some(0),
        },
    };
    let mut out = Vec::new();
    write_playback(&mut out, &status, false, false).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "Test song — First artist, Second artist  [test-vid001]\nplaying  2:03 / 6:00\n"
    );
    let mut out = Vec::new();
    write_playback(&mut out, &queue, false, false).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "> 1. Test song — First artist, Second artist  [test-vid001]\n"
    );
    for (response, path, expected) in [
        (status, "/position", json!(123.5)),
        (queue, "/current", json!(0)),
    ] {
        let mut out = Vec::new();
        write_playback(&mut out, &response, true, true).unwrap();
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value.pointer(path), Some(&expected));
    }
}

#[test]
fn remote_exit_codes_and_errors_match_the_cli_contract() {
    for (error, code, message) in [
        (ClientError::NotRunning, 4, "Kaset is not running."),
        (
            ClientError::Rejected(Error::NotSignedIn),
            2,
            "Not signed in. Sign in with the Kaset app first.",
        ),
        (
            ClientError::Rejected(Error::AuthExpired),
            3,
            "Session expired. Open the Kaset app to sign in again.",
        ),
    ] {
        let error = CliError::from(error);
        assert_eq!(error.exit_code(), ExitCode::from(code));
        assert_eq!(error.report(), message);
    }
}
