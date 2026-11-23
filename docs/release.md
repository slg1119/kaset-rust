# Releases

Push a version tag such as `v1.2.3` to run the Tauri release workflow.
The part after `v` becomes the bundle version. The macOS job builds a
universal DMG for Apple Silicon and Intel. The Windows job builds an x64
NSIS installer. Once both jobs succeed, the publish job attaches the
installers to the existing GitHub release or creates one for the tag.
Tag versions with a prerelease suffix create prereleases.

## Signing

- macOS defaults to ad-hoc signing. To sign and notarize, provide a
  Developer ID Application certificate through the existing
  `MACOS_CERTIFICATE` and `MACOS_CERTIFICATE_PWD` secrets. Notarization
  uses `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, and `APPLE_TEAM_ID`.
  Ad-hoc builds may require approval in macOS Privacy & Security.
  See [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/).
- Windows defaults to an unsigned installer. On a runner with a signing
  provider configured, the optional `KASET_WINDOWS_SIGN_COMMAND` repository
  variable supplies Tauri's signing command. Include `%1` for the binary
  path and keep credentials in the signing provider's secure storage.
  See [Tauri Windows signing](https://v2.tauri.app/distribute/sign/windows/).

## Distribution

Use the installers attached to GitHub Releases. The legacy Homebrew cask and
update feed were retired with the previous app. The Tauri release currently
uses manual downloads for updates.
