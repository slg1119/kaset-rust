# Playback

Kaset plays YouTube Music through a hidden Tauri WebView labelled `player`.
Rust validates the video ID, then navigates to the track's
`music.youtube.com/watch` page. The script observes the page's video element.
The remote window capability grants only `playback_event`.

Rust owns the queue and validates incoming events against the current
track. Track changes load a new watch page. An ended track advances the
queue according to the repeat and shuffle state. Radio can append tracks
when the queue runs low, with session and queue generation checks guarding
against stale responses.

The UI subscribes to `player://state` and `player://queue` via generated
bindings. The app also exposes playback through the platform media controls
and the same-user CLI connection. Closing the main window quits the app.
Sign-out clears the queue and closes the player.

Track-change notifications default to on and can be turned off in the app.
A timed lyric line can seek the current track when content is playing.

For bridge boundaries and the queue state machine, see
[ADR-0044](adr/0044-tauri-playback-bridge.md). For local remote control, see
[ADR-0046](adr/0046-cli-ipc.md).
