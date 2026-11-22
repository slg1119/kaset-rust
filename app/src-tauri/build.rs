fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "auth_status",
            "sign_in",
            "sign_out",
            "get_home",
        ]),
    ))
    .expect("failed to run tauri-build");
}
