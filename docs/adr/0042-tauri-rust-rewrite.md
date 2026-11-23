# ADR-0042: Rewrite Kaset as a Tauri app with a Rust core

## Status

Accepted — macOS verified by the M0 spike on 2026-11-22 (all four risks pass, findings linked below). Windows (WebView2) verification is pending hardware; revisit this ADR if it fails there.

## Context

Kaset is a Swift/SwiftUI app and only runs on macOS. We want Windows support
from one codebase. SwiftUI, App Intents, and Foundation Models are not
available outside Apple platforms.

## Decision

- Replace the Swift app with a Tauri 2 app in the same repository.
- Business logic (InnerTube client, parsers, auth/cookies, queue, player
  state) lives in Rust: `crates/ytm-core` (no Tauri dependency) and the Tauri
  shell in `app/src-tauri`.
- The Tauri executable is named `kaset-desktop` so the standalone CLI can
  use `kaset` without a Cargo binary name collision.
- The UI is React + TypeScript + Vite and never sees cookies.
- Playback keeps the approach of ADR-0001: a hidden WebView on
  music.youtube.com, bridged to Rust. On Windows this is WebView2.
- Swift sources remain as reference until the MVP is complete, then are
  deleted together with the Swift CI and Swift-specific AGENTS.md rules.

Design: `docs/superpowers/specs/2026-11-22-tauri-port-design.md`.
Spike findings: `docs/superpowers/spikes/2026-11-22-tauri-playback.md`.

## Consequences

### Positive
- macOS and Windows from one codebase.
- Parsers and client are unit-testable with plain `cargo test`.

### Negative
- Apple-only features are dropped: App Intents, Foundation Models command
  bar, Liquid Glass, AppleScript, extensions.
- The UI is no longer native SwiftUI.
- More third-party dependencies than the Swift app.
