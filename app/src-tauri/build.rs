fn main() {
    // Test executables need the manifest too, but tauri-build links it only into binaries.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
        let manifest = std::path::Path::new(&manifest_dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&[
                "auth_status",
                "sign_in",
                "sign_out",
                "get_feed",
                "get_feed_category",
                "get_feed_chip",
                "get_feed_continuation",
                "get_browse_page",
                "get_library",
                "get_browse_page_continuation",
                "get_artist",
                "get_artist_more",
                "play_artist_mix",
                "get_lyrics",
                "get_podcasts",
                "get_podcast_show",
                "get_podcast_episodes_continuation",
                "search",
                "search_continuation",
                "search_suggestions",
                "get_account",
                "window_vibrancy",
                "play_queue",
                "play_browse_queue",
                "toggle_playback",
                "next_track",
                "previous_track",
                "seek_to",
                "set_repeat",
                "set_shuffle",
                "player_snapshot",
                "playback_event",
            ]))
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("failed to run tauri-build");
}
