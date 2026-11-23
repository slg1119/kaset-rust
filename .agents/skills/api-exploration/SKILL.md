---
name: api-exploration
description: Use when changing a YouTube Music API request or parser, investigating a signed-in endpoint, or capturing a sanitized fixture with ytm-explorer.
metadata:
  short-description: Explore YouTube Music APIs
---

# API Exploration

1. Run `cargo run -p ytm-explorer -- --help` to list the commands.
2. For authenticated reads, point `KASET_COOKIE_FILE` at a private file
   outside the repo containing a Cookie header value. Leave the variable
   unset for guest requests.
3. Verify the live request and response shape before changing the client
   or parser. Extend `crates/ytm-explorer/src/main.rs` if the probe is missing.
4. Prefer summary commands. Where available, use `--out` to capture raw JSON
   privately outside the repo. Remove credentials, private IDs, and tokens
   before quoting excerpts or creating fixtures.
5. Exercise account changes against mock servers. Keep the explorer read-only
   and get explicit approval before sending any live write.
6. Record relevant findings in `docs/api-discovery.md` and put minimal,
   sanitized fixtures in `crates/ytm-core/tests/fixtures/`.

```bash
cargo run -p ytm-explorer -- search "jazz piano" --parse
cargo run -p ytm-explorer -- browse FEmusic_home --parse home
KASET_COOKIE_FILE=/path/outside/repo/cookie-header.txt \
  cargo run -p ytm-explorer -- library
```
