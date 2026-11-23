---
name: playback-webview-debugging
description: Use when investigating Tauri player navigation, DRM playback, bridge events, queue synchronization, or playback session recovery.
metadata:
  short-description: Debug the Tauri player WebView
---

# Playback WebView Debugging

1. Reproduce using `cd app && pnpm tauri dev`. Ask before running UI tests.
2. Start a track to create the hidden `player` window. Use WebView devtools
   in a local debug build, targeting the player document rather than the
   React window. On macOS, use Safari's Develop menu when the player
   document is listed. For a local diagnostic build, open the player
   window's devtools explicitly if the hidden window cannot be selected.
3. Compare timestamped navigation, bridge events, and Rust state. Confirm
   that the event belongs to the current video and that any async response
   matches the current session and queue generation.
4. Keep cookies, headers, and private response data out of traces and
   captures. Remove temporary instrumentation and rerun the reproduction.

Bridge and playback code live in `app/src-tauri/src/player/`. The remote
permissions are in `app/src-tauri/capabilities/player.json`.
See `docs/playback.md` and `docs/testing.md` for the architecture and
verification commands.
