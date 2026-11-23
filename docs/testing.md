# Testing

Run the workspace checks from the repository root.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Use `cargo test -p ytm-core` to test the API client and parsers in isolation.
Use sanitized JSON fixtures and mock HTTP responses for parser and request
tests, including requests that change account data.

After changing an IPC command, event, or shared IPC type, regenerate and
check the TypeScript bindings with `cargo test -p kaset bindings_are_current`.
Review any generated binding or permission changes before committing.

From `app/`, run:

```bash
pnpm lint
pnpm typecheck
pnpm test
```

Component tests use Vitest, Testing Library, `src/test/render.tsx` and
`mockCommands`. Cover loading and retry behavior in the screen and notices
for failed user actions.

## Desktop verification

Ask before running UI tests. For an approved desktop session, exercise
sign-in, playback and seeking, queue advancement, media keys, and the
track-change notification preference. Run the same checks on Windows
hardware when available. Hosted bundle builds cover packaging. Google
sign-in, OS secure storage, and DRM playback require a desktop session.
