---
name: dev-loop-packaging
description: Use when running Kaset locally, building a fresh Tauri installer, or reproducing an issue in a packaged build.
metadata:
  short-description: Run and package Kaset
---

# Dev Loop Packaging

From `app/`, install the locked dependencies and start the desktop dev loop.

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

The command starts Vite and runs Tauri. Use
`KASET_SESSION_STORE=file pnpm tauri dev` to isolate a debug session from the
OS secure store. Keep the generated session file private.

Package with `pnpm tauri build`. Use `--bundles dmg` on macOS or
`--bundles nsis` on Windows to select the installer. To build a universal
macOS DMG, install the `aarch64-apple-darwin` and `x86_64-apple-darwin`
Rust targets, then pass `--target universal-apple-darwin --bundles dmg`.
The bundle lands under the workspace Cargo `target` directory. See
`docs/release.md` for release signing and upload.

Ask before running UI tests. For compile-only or component checks, use
the commands in `docs/testing.md`.
