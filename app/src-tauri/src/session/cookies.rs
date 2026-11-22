//! Selecting cookies read from the WebView with `Webview::cookies()`.
//!
//! Never use `Webview::cookies_for_url`: wry's macOS implementation compares the
//! cookie domain to the URL host exactly, so `.youtube.com` cookies never match
//! `music.youtube.com` (see docs/superpowers/spikes/2026-11-22-tauri-playback.md).

use tauri::webview::Cookie;
use ytm_core::auth::CookieJar;

fn normalize(domain: &str) -> String {
    domain.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn is_under(domain: &str, root: &str) -> bool {
    domain == root || domain.ends_with(&format!(".{root}"))
}

/// `youtube.com` or any subdomain, with or without a leading dot.
pub fn is_youtube_domain(domain: &str) -> bool {
    is_under(&normalize(domain), "youtube.com")
}

/// Matches normalized country Google domains such as `accounts.google.co.kr`
/// or `google.com.au`, which the sign-in flow can visit to set cookies.
pub(crate) fn is_country_google(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let Some(google) = labels.iter().rposition(|label| *label == "google") else {
        return false;
    };
    let is_country_code = |tld: &str| tld.len() == 2 && tld.chars().all(|c| c.is_ascii_lowercase());
    match &labels[google + 1..] {
        [tld] => is_country_code(tld),
        [second, tld] => matches!(*second, "co" | "com") && is_country_code(tld),
        _ => false,
    }
}

/// Sign-out deletes cookies under youtube.com, google.com, and country Google domains.
pub fn is_session_domain(domain: &str) -> bool {
    let domain = normalize(domain);
    is_under(&domain, "youtube.com")
        || is_under(&domain, "google.com")
        || is_country_google(&domain)
}

/// Builds the API cookie jar from the youtube.com cookies in `cookies`.
pub fn youtube_cookie_jar(cookies: &[Cookie<'_>]) -> CookieJar {
    CookieJar::from_pairs(
        cookies
            .iter()
            .filter(|cookie| cookie.domain().is_some_and(is_youtube_domain))
            .map(|cookie| (cookie.name(), cookie.value())),
    )
}

/// The cookies sign-out must delete.
pub fn session_cookies(cookies: Vec<Cookie<'static>>) -> Vec<Cookie<'static>> {
    cookies
        .into_iter()
        .filter(|cookie| cookie.domain().is_some_and(is_session_domain))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ytm_core::secret::Secret;

    fn cookie(name: &'static str, value: &'static str, domain: &'static str) -> Cookie<'static> {
        Cookie::build((name, value)).domain(domain).build()
    }

    #[test]
    fn youtube_domain_matching() {
        assert!(is_youtube_domain(".youtube.com"));
        assert!(is_youtube_domain("youtube.com"));
        assert!(is_youtube_domain("music.youtube.com"));
        assert!(is_youtube_domain(".Music.YouTube.com"));
        assert!(!is_youtube_domain("notyoutube.com"));
        assert!(!is_youtube_domain("youtube.com.evil.example"));
        assert!(!is_youtube_domain(".google.com"));
        assert!(!is_youtube_domain(""));
    }

    #[test]
    fn session_domain_matching() {
        assert!(is_session_domain(".google.com"));
        assert!(is_session_domain("accounts.google.com"));
        assert!(is_session_domain(".google.co.kr"));
        assert!(is_session_domain("google.de"));
        assert!(is_session_domain("accounts.google.com.au"));
        assert!(is_session_domain(".youtube.com"));
        assert!(!is_session_domain("google.com.evil.example"));
        assert!(!is_session_domain("google.evil.example"));
        assert!(!is_session_domain("notgoogle.co.kr"));
        assert!(!is_session_domain("google.co.kr.evil.example"));
        assert!(!is_session_domain("example.com"));
    }

    #[test]
    fn jar_keeps_only_youtube_cookies() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("HSID", "test-cookie", "music.youtube.com"),
            cookie("SID", "test-google-cookie", ".google.com"),
            cookie("SAPISID", "test-country-cookie", ".google.co.kr"),
            cookie("OTHER", "test-cookie", "example.com"),
            Cookie::new("NODOMAIN", "test-cookie"),
        ];
        let jar = youtube_cookie_jar(&cookies);
        assert_eq!(
            jar.header().expose(),
            "SAPISID=test-sapisid; HSID=test-cookie"
        );
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn empty_webview_cookies_have_no_sapisid() {
        assert!(youtube_cookie_jar(&[]).sapisid().is_none());
    }

    #[test]
    fn jar_prefers_secure_sapisid_from_webview_cookies() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("__Secure-3PAPISID", "test-secure-sapisid", ".youtube.com"),
        ];
        assert_eq!(
            youtube_cookie_jar(&cookies).sapisid(),
            Some(Secret::new("test-secure-sapisid"))
        );
    }

    #[test]
    fn session_cookies_cover_google_and_youtube() {
        let cookies = vec![
            cookie("SAPISID", "test-sapisid", ".youtube.com"),
            cookie("SID", "test-google-cookie", ".google.com"),
            cookie("HSID", "test-country-cookie", ".google.co.kr"),
            cookie("OTHER", "test-cookie", "example.com"),
        ];
        let names: Vec<_> = session_cookies(cookies)
            .iter()
            .map(|cookie| cookie.name().to_owned())
            .collect();
        assert_eq!(names, ["SAPISID", "SID", "HSID"]);
    }
}
