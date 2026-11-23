# ADR-0047: Remove the Swift app

## Status

Accepted (2026-11-23).

## Context

ADR-0042 approved a full replacement of the macOS app with a Tauri 2, Rust,
and React desktop app. The Tauri app now covers the MVP and ships a
standalone CLI with local playback remote control. The original
implementation served as a reference during the port.

## Decision

Remove the former app, its tests, packaging scripts, and CI. Use the Tauri
workspace as the product source and publish a universal macOS DMG and a
Windows NSIS installer on version tags. Retire the previous update feed and
Homebrew cask. Retain the ADRs and playback spike findings as historical
records.

## Consequences

Developers build and test the desktop app through Cargo and pnpm. Users
download installers from GitHub Releases. README tracks features still to
be ported. Windows hardware verification and live signed-in API verification
remain open.
