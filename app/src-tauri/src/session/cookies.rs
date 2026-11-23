//! Selecting cookies read from the WebView with `Webview::cookies()`.
//!
//! Never use `Webview::cookies_for_url`: wry's macOS implementation compares the
//! cookie domain to the URL host exactly, so `.youtube.com` cookies never match
//! `music.youtube.com` (see docs/superpowers/spikes/2026-11-22-tauri-playback.md).

use std::collections::BTreeMap;

use tauri::webview::Cookie;
use tauri::webview::cookie::time::OffsetDateTime;
use ytm_core::auth::CookieJar;

fn normalize(domain: &str) -> String {
    domain.trim().trim_start_matches('.').to_ascii_lowercase()
}

pub(crate) fn is_under(domain: &str, root: &str) -> bool {
    domain == root || domain.ends_with(&format!(".{root}"))
}

/// The normalized length of `youtube.com` or any of its subdomains.
fn youtube_domain_len(domain: &str) -> Option<usize> {
    let domain = normalize(domain);
    is_under(&domain, "youtube.com").then_some(domain.len())
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

/// Builds a name-sorted API jar, preferring the most specific YouTube domain.
pub fn youtube_cookie_jar(cookies: &[Cookie<'_>]) -> CookieJar {
    let mut by_name = BTreeMap::new();
    for cookie in cookies {
        let Some(domain_len) = cookie.domain().and_then(youtube_domain_len) else {
            continue;
        };
        let selected = by_name
            .entry(cookie.name().trim())
            .or_insert((domain_len, cookie.value()));
        if domain_len > selected.0 {
            *selected = (domain_len, cookie.value());
        }
    }
    CookieJar::from_pairs(by_name.into_iter().map(|(name, (_, value))| (name, value)))
}

pub(crate) const MUSIC_HOST: &str = "music.youtube.com";
pub(crate) const YOUTUBE_DOMAIN: &str = "youtube.com";

/// Matches an auth cookie readable by the player page. An exact expiry
/// identifies a cookie written by our restore, even if Wry normalized its scope.
pub(crate) fn has_player_auth(
    cookies: &[Cookie<'_>],
    domain: &str,
    restored_expiry: Option<OffsetDateTime>,
) -> bool {
    let now = OffsetDateTime::now_utc();
    cookies.iter().any(|cookie| {
        let expiry = cookie.expires_datetime();
        matches!(cookie.name(), "SAPISID" | "__Secure-3PAPISID")
            && !cookie.value().is_empty()
            && cookie.http_only() != Some(true)
            && cookie
                .domain()
                .is_some_and(|value| normalize(value) == domain)
            && matches!(cookie.path(), Some("/" | "/watch"))
            && expiry.is_none_or(|expires| expires > now)
            && restored_expiry.is_none_or(|expected| expiry == Some(expected))
    })
}

/// wry passes Cookie::domain() to WebKit and WebView2, and that accessor strips
/// one leading dot. Retain the native domain-cookie marker across that boundary.
pub(crate) fn native_domain_cookie(domain: &str) -> String {
    format!("..{domain}")
}

/// Rebuilds WebView cookie attributes lost when the session became an API header.
pub(crate) fn player_cookies(
    jar: &CookieJar,
    expires: OffsetDateTime,
) -> impl Iterator<Item = Cookie<'_>> {
    jar.pairs().filter_map(move |(name, value)| {
        let valid_name = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        let valid_value = value
            .bytes()
            .all(|b| matches!(b, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e));
        if !valid_name || !valid_value {
            return None;
        }
        let domain = if name.starts_with("__Host-") {
            // Native stores require a host. No leading dot means host-only.
            MUSIC_HOST.to_owned()
        } else {
            native_domain_cookie(YOUTUBE_DOMAIN)
        };
        // The page computes its request signature from these browser cookies.
        let signs_requests = matches!(name, "SAPISID" | "__Secure-1PAPISID" | "__Secure-3PAPISID");
        let builder = Cookie::build((name, value))
            .domain(domain)
            .path("/")
            .secure(true)
            .expires(expires);
        // Foundation checks for the presence of HttpOnly, even when Wry
        // passes the string "FALSE". Omit the attribute for signing cookies.
        let builder = if signs_requests {
            builder
        } else {
            builder.http_only(true)
        };
        Some(builder.build())
    })
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
    use tauri::webview::cookie::time::Duration;
    use ytm_core::secret::Secret;

    fn cookie(name: &'static str, value: &'static str, domain: &'static str) -> Cookie<'static> {
        Cookie::build((name, value)).domain(domain).build()
    }

    #[test]
    fn restores_header_values_with_native_domain_and_prefix_rules() {
        let jar = CookieJar::from_header(
            "SAPISID=test=auth; __Secure-3PSID=test-secure; __Host-example=test-host; invalid name=test; malformed",
        );
        let expiry = OffsetDateTime::now_utc() + Duration::days(365);
        let cookies: Vec<_> = player_cookies(&jar, expiry).collect();
        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies[0].name(), "SAPISID");
        assert_eq!(cookies[0].value(), "test=auth");
        assert_eq!(cookies[0].domain(), Some(".youtube.com"));
        assert_eq!(cookies[1].domain(), Some(".youtube.com"));
        assert_eq!(cookies[2].domain(), Some(MUSIC_HOST));
        assert_eq!(cookies[0].http_only(), None);
        assert_eq!(cookies[1].http_only(), Some(true));
        assert_eq!(cookies[2].http_only(), Some(true));
        for cookie in &cookies {
            assert_eq!(cookie.path(), Some("/"));
            assert_eq!(cookie.secure(), Some(true));
            assert_eq!(cookie.expires_datetime(), Some(expiry));
        }
    }

    #[test]
    fn auth_detection_ignores_unusable_cookies() {
        let mut cookies = vec![
            cookie("SAPISID", "test-google", ".google.com"),
            cookie("SAPISID", "test-sibling", "studio.youtube.com"),
            cookie("SAPISID", "test-other-path", ".youtube.com"),
            cookie("SAPISID", "", ".youtube.com"),
            cookie("SAPISID", "test-expired", ".youtube.com"),
        ];
        cookies[2].set_path("/other");
        cookies[3].set_path("/");
        cookies[4].set_path("/");
        cookies[4].set_expires(OffsetDateTime::now_utc() - Duration::days(1));
        assert!(!has_player_auth(&cookies, MUSIC_HOST, None));
        assert!(!has_player_auth(&cookies, YOUTUBE_DOMAIN, None));
        let mut auth = cookie("__Secure-3PAPISID", "test-auth", ".youtube.com");
        auth.set_path("/");
        auth.set_http_only(true);
        cookies.push(auth.clone());
        assert!(!has_player_auth(&cookies, YOUTUBE_DOMAIN, None));
        auth.set_http_only(false);
        cookies.push(auth);
        assert!(has_player_auth(&cookies, YOUTUBE_DOMAIN, None));
        assert!(!has_player_auth(&cookies, MUSIC_HOST, None));
    }

    #[test]
    fn youtube_domain_matching() {
        assert!(youtube_domain_len(".youtube.com").is_some());
        assert!(youtube_domain_len("youtube.com").is_some());
        assert!(youtube_domain_len("music.youtube.com").is_some());
        assert!(youtube_domain_len(".Music.YouTube.com").is_some());
        assert!(youtube_domain_len("notyoutube.com").is_none());
        assert!(youtube_domain_len("youtube.com.evil.example").is_none());
        assert!(youtube_domain_len(".google.com").is_none());
        assert!(youtube_domain_len("").is_none());
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
            "HSID=test-cookie; SAPISID=test-sapisid"
        );
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn jar_keeps_the_most_specific_domain_once_and_sorts_names() {
        let mut cookies = [
            cookie("SAPISID", "test-parent", ".youtube.com"),
            cookie("SAPISID", "test-music", "music.youtube.com"),
            cookie("HSID", "test-cookie", ".youtube.com"),
        ];
        let expected = Secret::new("HSID=test-cookie; SAPISID=test-music");
        assert_eq!(youtube_cookie_jar(&cookies).header(), &expected);
        cookies.reverse();
        assert_eq!(youtube_cookie_jar(&cookies).header(), &expected);
    }

    #[test]
    fn jar_keeps_the_first_cookie_when_normalized_domain_lengths_tie() {
        let cookies = [
            cookie("SAPISID", "test-first", "music.youtube.com"),
            cookie("SAPISID", "test-second", ".Music.YouTube.com"),
        ];
        assert_eq!(
            youtube_cookie_jar(&cookies).header(),
            &Secret::new("SAPISID=test-first")
        );
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
