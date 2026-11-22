fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "open_login",
            "open_player",
            "dump_cookies",
            "playback_event",
        ]),
    ))
    .expect("failed to run tauri-build");
}
