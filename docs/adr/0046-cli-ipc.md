# ADR-0046: Local playback IPC for the Kaset CLI

## Status

Accepted as part of the Tauri port's M5b feature parity wave.

## Context

The standalone `kaset` CLI can browse with the saved session. Playback and its
queue live in the desktop app's player service and hidden WebView (ADR-0044).
The CLI needs a local channel to the running player.

## Decision

- Share a Tauri-independent version 1 protocol in `kaset-ipc`. Each connection
  carries one newline-terminated JSON request and one response. Requests have a
  `v` and `cmd` field. The command set is `status`, `play_pause`, `play`,
  `pause`, `next`, `previous`, `seek`, `queue`, `play_video`, `play_page`,
  `shuffle`, and `repeat`. A response has a `result` discriminator with an
  acknowledgement, status, queue, or a fixed error code.
- Cap request lines at 4 KiB before JSON parsing and close the connection after
  each response, including validation errors. The listener bounds concurrent
  connections and applies read and write deadlines. The CLI bounds the reply
  size and total wait.
- On macOS, listen at `$TMPDIR/kaset-<uid>/kaset.sock`. The app requires an
  owned, private 0700 directory and applies 0600 permissions to the socket.
  A lock prevents a second listener from removing a live endpoint. Startup
  removes an owned stale socket. Both sides check the peer's effective UID.
- On Windows, listen at `\\.\pipe\kaset-<username>` and reject remote clients.
  The pipe owner and sole DACL grant use the current process token's user SID.
  The CLI checks the pipe owner before sending a command and opens the pipe at
  identification impersonation level. The app reserves the first instance to
  avoid attaching to an existing pipe name.
- The app authenticates against its session and applies commands through the
  same player service as the UI. `play_video` validates the 11-character ID,
  uses it as the initial title, and starts with one track. The existing radio
  policy fills the tail. `play_page` uses the existing signed-in browse
  command's loader and ID rules. A reply from a previous session or an
  earlier queue generation cannot replace current playback.
- `now --json` and `queue --json` expose playback metadata and the zero-based
  current queue index for scripts. Playback commands use the app session and
  require only a connection to the app. A missing endpoint yields exit code 4.

## Threat model

Every local process running as the same OS user can read playback metadata and
change the current queue. OS administrators may have privileges beyond this
boundary. Endpoint discovery can be disrupted by path or name collisions.
Startup fails closed on unexpected endpoint ownership or type and the app
continues to run if the listener cannot start.

The protocol accepts a fixed command set, typed fields, validated IDs, and
bounded input. It carries no credentials, account writes, stream URLs,
arbitrary URLs, or JavaScript. The WebView's existing Tauri capability and
navigation checks continue to govern playback separately.

Windows access control follows the platform's named-pipe security model.
[Microsoft Learn: Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
