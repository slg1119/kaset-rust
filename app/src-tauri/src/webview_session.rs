//! Moves WebView cookie reads and session store I/O off the async workers.
//!
//! `Webview::cookies()` blocks until the main thread answers (and deadlocks on
//! Windows when called from the main thread), and keyring calls block on the
//! OS store, so both run on Tauri's blocking pool.

use tauri::webview::cookie::time::{Duration, OffsetDateTime};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::error::AppError;
use crate::session::cookies::{
    MUSIC_HOST, YOUTUBE_DOMAIN, has_player_auth, native_domain_cookie, player_cookies,
    session_cookies, youtube_cookie_jar,
};
use crate::state::AppState;

/// Runs `work` on the blocking pool.
pub async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(AppError::from)
}

/// Remembers the expiry confirmed after writing the native domain cookie.
/// A parent-domain read alone is ambiguous because Wry loses the leading dot.
pub(crate) struct RestoredPlayerAuth {
    epoch: u64,
    expires: OffsetDateTime,
}

fn scoped_player_auth(
    cookies: &[tauri::webview::Cookie<'_>],
    epoch: u64,
    restored: Option<&RestoredPlayerAuth>,
) -> bool {
    has_player_auth(cookies, MUSIC_HOST, None)
        || restored.is_some_and(|proof| {
            proof.epoch == epoch && has_player_auth(cookies, YOUTUBE_DOMAIN, Some(proof.expires))
        })
}

/// Seed the actual player store before navigating. The caller holds the player
/// lock, which also serializes sign-in, sign-out, and API expiry.
pub(crate) async fn restore_player_cookies(
    window: &WebviewWindow,
    restored: Option<RestoredPlayerAuth>,
) -> Result<Option<RestoredPlayerAuth>, AppError> {
    let app = window.app_handle().clone();
    let epoch = app.state::<AppState>().session.epoch();
    let window = window.clone();
    blocking(move || {
        let state = app.state::<AppState>();
        if !state.session.is_signed_in_at(epoch) {
            return Err(AppError::not_signed_in());
        }
        let restore_error =
            || AppError::internal("Could not restore the YouTube Music session in the player.");
        let live = window.cookies().map_err(|_| restore_error())?;
        let restored = if scoped_player_auth(&live, epoch, restored.as_ref()) {
            restored
        } else {
            let jar = state
                .session
                .cookies_for_epoch(epoch)
                .ok_or_else(AppError::not_signed_in)?;
            // Native stores return expiration at second precision.
            let expires = (OffsetDateTime::now_utc() + Duration::days(365))
                .replace_nanosecond(0)
                .expect("zero nanoseconds is valid");
            let mut count = 0;
            for cookie in player_cookies(&jar, expires) {
                if !state.session.is_signed_in_at(epoch) {
                    return Err(AppError::not_signed_in());
                }
                window.set_cookie(cookie).map_err(|_| restore_error())?;
                count += 1;
            }
            // set_cookie queues the native write. Read the store back before
            // allowing a watch request to use the restored signing cookie.
            let live = window.cookies().map_err(|_| restore_error())?;
            if !has_player_auth(&live, YOUTUBE_DOMAIN, Some(expires)) {
                return Err(restore_error());
            }
            tracing::debug!(count, "restored player WebView cookies");
            Some(RestoredPlayerAuth { epoch, expires })
        };
        if !state.session.is_signed_in_at(epoch) {
            return Err(AppError::not_signed_in());
        }
        Ok(restored)
    })
    .await?
}

/// Deletes YouTube and Google session cookies from the shared WebView store,
/// re-reading up to three times like the Swift app's cookie clearer. Call on
/// the blocking pool.
pub(crate) fn clear_webview_cookies(window: &WebviewWindow, reason: &'static str) {
    for _ in 0..3 {
        let remaining = match window.cookies() {
            Ok(cookies) => session_cookies(cookies),
            Err(error) => {
                tracing::warn!(%error, reason, "could not read cookies for WebView cleanup");
                return;
            }
        };
        if remaining.is_empty() {
            return;
        }
        for mut cookie in remaining {
            if let Err(error) = window.delete_cookie(cookie.clone()) {
                tracing::warn!(%error, reason, "could not delete a WebView cookie");
            }
            // The cookie accessor normalizes away the native domain marker.
            // Both scopes are in the sign-out allowlist.
            if !cookie.name().starts_with("__Host-")
                && let Some(domain) = cookie.domain()
            {
                cookie.set_domain(native_domain_cookie(domain));
                if let Err(error) = window.delete_cookie(cookie) {
                    tracing::warn!(%error, reason, "could not delete a WebView domain cookie");
                }
            }
        }
    }
    tracing::warn!(reason, "Google cookies remained after WebView cleanup");
}

/// Reads the shared cookie store through the `label` window and passes the
/// YouTube cookies to `Session::refresh_if`. Runs at launch and after every
/// player page load. Returns true when the in-memory cookies changed.
pub async fn refresh_session_from_webview(
    app: &AppHandle,
    label: &str,
    reason: &'static str,
) -> bool {
    let Some(window) = app.get_webview_window(label) else {
        return false;
    };
    let task_app = app.clone();
    let result = blocking(move || {
        let state = task_app.state::<AppState>();
        let epoch = state.session.epoch();
        let cookies = window.cookies()?;
        Ok::<_, tauri::Error>(
            state
                .session
                .refresh_if(epoch, youtube_cookie_jar(&cookies)),
        )
    })
    .await;
    match result {
        Ok(Ok(changed)) => {
            if changed {
                tracing::info!(reason, "refreshed session cookies from the WebView");
            }
            changed
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, reason, "could not read WebView cookies");
            false
        }
        Err(error) => {
            tracing::warn!(%error, reason, "cookie refresh task failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::webview::Cookie;

    #[test]
    fn an_unconfirmed_parent_cookie_cannot_suppress_recovery() {
        let mut parent = Cookie::build(("SAPISID", "test-cookie"))
            .domain(YOUTUBE_DOMAIN)
            .path("/")
            .build();
        let music = Cookie::build(("SAPISID", "test-cookie"))
            .domain(MUSIC_HOST)
            .path("/")
            .build();
        assert!(scoped_player_auth(&[music], 1, None));
        assert!(!scoped_player_auth(std::slice::from_ref(&parent), 1, None));

        let expires = OffsetDateTime::now_utc() + Duration::days(365);
        let restored = RestoredPlayerAuth { epoch: 1, expires };
        assert!(!scoped_player_auth(
            std::slice::from_ref(&parent),
            1,
            Some(&restored)
        ));
        parent.set_expires(expires);
        assert!(scoped_player_auth(
            std::slice::from_ref(&parent),
            1,
            Some(&restored)
        ));
        assert!(!scoped_player_auth(
            std::slice::from_ref(&parent),
            2,
            Some(&restored)
        ));
        assert!(!scoped_player_auth(&[], 1, Some(&restored)));
    }
}
