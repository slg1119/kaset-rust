# Architecture

React renders the desktop UI, Tauri hosts the windows and native integration,
and Rust owns the YouTube Music API, session, queue, and playback state.

| Component | Responsibility |
| --- | --- |
| `app/src` | React screens, state-based navigation, TanStack Query cache |
| `app/src-tauri` | Typed commands and events, WebViews, playback, OS media controls |
| `crates/ytm-core` | InnerTube request signing, HTTP client, parsers, and models |
| `crates/ytm-explorer` | Read-only API response probes |
| `crates/kaset-session` | Secure session storage shared by app and CLI |
| `crates/kaset-ipc` | Typed same-user protocol and transport for playback commands |
| `crates/kaset-cli` | Standalone catalog queries and app playback remote control |

## Data flow

1. Google sign-in runs in a visible WebView. The Tauri backend reads and
   filters its cookies, then persists the session in the OS secure store.
2. React uses the generated `commands` bindings to request data. Rust calls
   InnerTube and returns parsed models. React caches these with TanStack
   Query and keeps retry controls with the affected screen.
3. Rust owns the queue and drives the hidden music player. The player bridge
   reports progress and track completion. Tauri publishes player state and
   queue snapshots to React and updates the OS media controls.
4. CLI browsing uses the shared session store and API client directly.
   Playback commands go to the running app via a Unix-domain socket on macOS
   or a named pipe on Windows.

See the decisions on [session and IPC](adr/0043-tauri-app-session-and-ipc.md),
[playback](adr/0044-tauri-playback-bridge.md),
[the UI shell](adr/0045-tauri-ui-shell.md), and
[CLI playback](adr/0046-cli-ipc.md).
