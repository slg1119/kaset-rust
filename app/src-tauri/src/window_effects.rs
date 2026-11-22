//! Native translucency behind the main window's web content: macOS sidebar
//! vibrancy, Windows 11 Mica. The window is transparent (`tauri.conf.json`)
//! and the page stays opaque except the sidebar, and only when an effect
//! applied, so unsupported systems (Windows 10) show a solid sidebar.

use tauri::WebviewWindow;

/// Managed state: whether `apply` succeeded for the main window.
pub struct WindowEffects {
    pub vibrancy: bool,
}

/// Applies the platform effect to `window`. Call on the main thread (Tauri
/// `setup`). Returns whether the effect is active.
pub fn apply(window: &WebviewWindow) -> bool {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy};
        match apply_vibrancy(
            window,
            NSVisualEffectMaterial::Sidebar,
            Some(NSVisualEffectState::FollowsWindowActiveState),
            None,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not apply window vibrancy");
                false
            }
        }
    }
    #[cfg(windows)]
    {
        match window_vibrancy::apply_mica(window, Some(true)) {
            Ok(()) => true,
            // Windows 10: Mica needs Windows 11; keep the solid sidebar.
            Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => false,
            Err(error) => {
                tracing::warn!(%error, "could not apply Mica");
                false
            }
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = window;
        false
    }
}
