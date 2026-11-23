# Contributing to Kaset

## Getting started

Install stable Rust, Node.js 24, the pnpm version in `app/package.json`, and
the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
On macOS this includes Apple's command-line build tools. On Windows it
includes the MSVC build tools and WebView2.

```bash
cd app
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run `pnpm tauri build` from `app/` for a packaged build. The
[release guide](docs/release.md) describes the tag workflow and signing.

## Project structure

| Path | Purpose |
| --- | --- |
| `app/src` | React screens, navigation, and queries |
| `app/src-tauri` | Tauri commands, sign-in, playback, and OS integration |
| `crates/ytm-core` | InnerTube client, parsers, and models |
| `crates/ytm-explorer` | Read-only API probes |
| `crates/kaset-session` | Shared session persistence |
| `crates/kaset-ipc` | Local playback IPC protocol and transport |
| `crates/kaset-cli` | Standalone command-line client and app remote control |

See [architecture](docs/architecture.md), [playback](docs/playback.md), and
[API discovery](docs/api-discovery.md) for more detail.

## Coding and verification

Follow [AGENTS.md](AGENTS.md) for credential handling, API verification, IPC
bindings, and review. Keep changes focused and update the relevant docs and
[keyboard shortcut list](docs/keyboard-shortcuts.md) when behavior changes.

Run the workspace checks from the repository root.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run the frontend checks from `app/`.

```bash
pnpm lint
pnpm typecheck
pnpm test
```

Record the commands and results in your PR. See the [testing guide](docs/testing.md)
for focused checks and desktop verification.

## AI-Assisted Contributions & Prompt Requests

We embrace AI-assisted development! Whether you use GitHub Copilot, Claude, Cursor, or other AI tools, we welcome contributions that leverage these capabilities.

### What is a Prompt Request?

A **prompt request** is a contribution where you share the AI prompt that generates code, rather than (or in addition to) the code itself. This approach:

- **Captures intent** — The prompt often explains _why_ better than a code diff
- **Enables review before implementation** — Maintainers can validate the approach
- **Supports iteration** — Prompts can be refined before code is generated
- **Improves reproducibility** — Anyone can run the prompt to verify results

### Contributing with AI Assistance

#### Option 1: Traditional PR with AI Prompt Disclosure

Submit code as usual, but include the AI prompt in the PR template's "AI Prompt" section. This helps reviewers understand your approach and intent.

#### Option 2: Prompt Request (Prompt-Only)

Create an issue using the **Prompt Request** template if you:

- Have a well-crafted prompt but haven't run it yet
- Want feedback on your approach before implementation
- Prefer maintainers to run and merge the prompt themselves

### Best Practices for AI Prompts

1. **Be specific** — Include file paths, function names, and concrete requirements
2. **Reference project conventions** — Mention AGENTS.md and relevant patterns
3. **Define acceptance criteria** — How will we know it worked?
4. **Include context** — Link to issues, docs, or examples
5. **Test locally when possible** — Verify the prompt produces working code
