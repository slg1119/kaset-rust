# YouTube Music API Discovery

Use `ytm-explorer` to check YouTube Music InnerTube request and response
shapes before changing the client or parsers.

```bash
cargo run -p ytm-explorer -- --help
cargo run -p ytm-explorer -- search "jazz piano" --parse
cargo run -p ytm-explorer -- feed FEmusic_explore
```

For signed-in reads, set `KASET_COOKIE_FILE` to the path of a private file
outside the repository containing a single Cookie header value. Leave it
unset for a signed-out probe. The explorer obtains the API key from YouTube
Music unless a key has been explicitly configured. Keep both credentials and
raw responses private. Prefer summary commands where available.

```bash
KASET_COOKIE_FILE=/path/outside/repo/cookie-header.txt \
  cargo run -p ytm-explorer -- library
```

Use the subcommand's `--help` to find its options. Where available, use
`--out` to write a raw response to a private file outside the repository.
Remove account identifiers, tokens, and any other private values before
turning captures into fixtures. Store minimal fixtures with placeholders in
`crates/ytm-core/tests/fixtures/`.

Requests and authorization live in `crates/ytm-core/src/client.rs` and
`auth.rs`. Parsers live in `crates/ytm-core/src/parsers/`. The explorer
provides read-only probes. Use mock servers when implementing likes, library
edits, playlist edits, or subscriptions.

Earlier protocol investigations are in this document's Git history.
