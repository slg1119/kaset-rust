# Kaset

Kaset is a YouTube Music desktop app for macOS and Windows built with Tauri 2,
Rust, and React.

## Features

- Sign in with Google to browse YouTube Music home, explore, charts, moods,
  new releases, history, and your library.
- Search for songs, albums, artists, playlists, and podcasts. Open detail
  pages, play episodes, and follow timed lyrics when available.
- Manage your queue, start radio, shuffle and repeat, and control playback
  from the app or your system media keys. Track-change notifications can be
  turned off in the app.
- Like songs, save albums and playlists, edit playlists, and subscribe to
  artists and podcasts.

## Install or run from source

Download the macOS universal DMG or Windows x64 NSIS installer from the
repository's [GitHub releases](https://github.com/slg1119/kaset-rust/releases).
See the [release guide](docs/release.md) for packaging and signing details.

With Rust, Node.js, pnpm, and the platform build tools installed, run:

```bash
cd app && pnpm install && pnpm tauri dev
```

Use `pnpm tauri build` from `app/` to build an installer. See the
[contributor guide](CONTRIBUTING.md) for setup and checks.

## Not yet ported

- AppleScript, the custom URL scheme, and Siri / App Shortcuts
  (`docs/applescript.md`, `docs/url-scheme.md`, `docs/shortcuts.md`).
- Regular YouTube and YouTube Music video mode
  (`docs/youtube.md`, `docs/video.md`).
- WebView extensions (`docs/extensions.md`), scrobbling, and Liquid Glass.

## Command line

Build and install the standalone CLI from the repository on macOS or Windows.

```bash
cargo install --locked --path crates/kaset-cli
```

Sign in once with the Tauri version of the Kaset app. The browsing commands
read the session from your OS secure store and can run with the app closed.
On macOS, Keychain may prompt the first time the CLI reads the session saved
by the app. Allow access to use browsing commands.

```bash
kaset search "jazz piano" --filter songs --limit 5
kaset home
kaset explore
kaset charts
kaset moods
kaset new
kaset history
kaset page MPREb_test
kaset radio test-vid001
kaset whoami --json
```

Use the browse or video IDs printed in brackets as inputs to `page` or
`radio`. Search filtering and limits apply to the first result page.
Use `--json` on any command for parsed data. Internal pagination and browse
parameter fields appear as `null`. Set `NO_COLOR` to a non-empty value to
disable terminal styling.

For a debug app using `KASET_SESSION_STORE=file`, build the CLI in debug mode
with `cargo install --debug --locked --path crates/kaset-cli`. Point it to the
app's `dev-session.txt` with `--session-file <path>`, or set both
`KASET_SESSION_STORE=file` and `KASET_SESSION_FILE=<path>` for the CLI. Release
builds read the OS secure store. A missing session exits with status 2 and an
expired session exits with status 3.

Playback commands control the running app using its active session.

```bash
kaset now --json
kaset play
kaset play test-vid001
kaset play --page MPREb_test
kaset pause
kaset toggle
kaset next
kaset prev
kaset seek 1:30
kaset queue --json
kaset shuffle on
kaset repeat all
```

`now` and `queue` print text by default. `queue --json` uses a zero-based
current-track index. Playback commands exit with status 4 when the app is closed.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, architecture, and coding guidelines.

We welcome AI-assisted contributions! You can submit traditional PRs or **prompt requests** — share the AI prompt that generates your changes, and maintainers can review the intent before running the code. See the [AI-Assisted Contributions](CONTRIBUTING.md#ai-assisted-contributions--prompt-requests) section for details.

## Disclaimer

Kaset is an unofficial application and not affiliated with YouTube or Google Inc. in any way. "YouTube", "YouTube Music" and the "YouTube Logo" are registered trademarks of Google Inc.
