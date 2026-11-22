pub mod commands;
pub mod error;
pub mod events;
pub mod login;
pub mod player;
pub mod session;
pub mod state;
pub mod webview_session;

use tauri::Manager;
use tauri_specta::{Builder, collect_commands, collect_events};

use crate::session::Session;
use crate::session::store::{STORE_ENV, store_for, store_kind};
use crate::state::{AppState, LAUNCH_REFRESH_TIMEOUT};
use crate::webview_session::{blocking, refresh_session_from_webview};

/// Commands and events exposed to the UI. `bindings_are_current` exports
/// them to `app/src/bindings.ts`.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::auth_status,
            commands::sign_in,
            commands::sign_out,
            commands::get_home,
        ])
        .events(collect_events![events::AuthChanged])
        .disable_serde_phases()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kaset_lib=info,ytm_core=info".into()),
        )
        .init();

    let builder = specta_builder();
    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let data_dir = app.path().app_data_dir()?;
            let kind = store_kind(
                std::env::var(STORE_ENV).ok().as_deref(),
                cfg!(debug_assertions),
            );
            tracing::info!(store = ?kind, "session store selected");
            // Restore before accepting commands. Store I/O runs on the blocking pool.
            let session = tauri::async_runtime::block_on(blocking(move || {
                let session = Session::new(store_for(kind, &data_dir));
                let status = session.restore();
                tracing::info!(?status, "session restored");
                session
            }))?;
            app.manage(AppState::new(session));
            refresh_cookies_from_webview(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Kaset");
}

/// Gives launch cookie rotation a bounded window before opening the API gate.
fn refresh_cookies_from_webview(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        if tokio::time::timeout(
            LAUNCH_REFRESH_TIMEOUT,
            refresh_session_from_webview(&app, "main", "launch"),
        )
        .await
        .is_err()
        {
            tracing::warn!("launch WebView cookie refresh timed out");
        }
        app.state::<AppState>().launch.open();
    });
}

#[cfg(test)]
mod tests {
    use specta_typescript::Typescript;

    /// Regenerates `app/src/bindings.ts`. CI fails if the committed file differs.
    #[test]
    fn bindings_are_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");
        super::specta_builder()
            .mocks()
            .export(Typescript::default().header("// @ts-nocheck"), path)
            .expect("failed to export bindings");
    }
}
