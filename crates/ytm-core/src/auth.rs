use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use crate::secret::Secret;

use sha1::{Digest, Sha1};

/// SAPISIDHASH value for an InnerTube request. The origin is part of the hash,
/// so a value computed for music.youtube.com is not valid for www.youtube.com.
pub fn sapisid_hash(sapisid: &str, origin: &str, timestamp: u64) -> String {
    let digest = Sha1::digest(format!("{timestamp} {sapisid} {origin}").as_bytes());
    format!("{timestamp}_{}", hex::encode(digest))
}

/// A `Cookie` header for youtube.com. Values stay inside `Secret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieJar {
    header: Secret,
}

impl CookieJar {
    pub fn from_header(header: &str) -> Self {
        Self {
            header: Secret::new(header.trim()),
        }
    }

    /// Builds a header from (name, value) pairs, e.g. cookies read from a WebView.
    /// Skips pairs that cannot be represented in a `Cookie` header: an empty name,
    /// a name containing `=` or `;`, or a value containing `;`.
    pub fn from_pairs<I, N, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (N, V)>,
        N: AsRef<str>,
        V: AsRef<str>,
    {
        let header = pairs
            .into_iter()
            .filter_map(|(name, value)| {
                let name = name.as_ref().trim();
                let value = value.as_ref().trim();
                let valid = !name.is_empty() && !name.contains(['=', ';']) && !value.contains(';');
                valid.then(|| format!("{name}={value}"))
            })
            .collect::<Vec<_>>()
            .join("; ");
        Self {
            header: Secret::new(header),
        }
    }

    pub fn header(&self) -> &Secret {
        &self.header
    }

    /// Iterates over the header's (name, value) pairs. Values are sensitive.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.header
            .expose()
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
    }

    /// Returns `__Secure-3PAPISID` when present, falling back to `SAPISID`.
    pub fn sapisid(&self) -> Option<Secret> {
        self.pairs()
            .find(|(name, _)| *name == "__Secure-3PAPISID")
            .or_else(|| self.pairs().find(|(name, _)| *name == "SAPISID"))
            .map(|(_, value)| Secret::new(value))
    }
}

/// A cookie jar shared between the client and its owner. Cloning shares the
/// same slot, so `set` on any clone changes the cookies every clone sends.
#[derive(Clone, Default)]
pub struct SharedCookies(Arc<RwLock<Option<CookieJar>>>);

impl SharedCookies {
    pub fn new(jar: Option<CookieJar>) -> Self {
        Self(Arc::new(RwLock::new(jar)))
    }

    pub fn get(&self) -> Option<CookieJar> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, jar: Option<CookieJar>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = jar;
    }
}

impl fmt::Debug for SharedCookies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = if self.get().is_some() { "set" } else { "empty" };
        write!(f, "SharedCookies({state})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sapisid_hash_matches_known_value() {
        // sha1("1700000000 test-sapisid https://music.youtube.com")
        assert_eq!(
            sapisid_hash("test-sapisid", "https://music.youtube.com", 1_700_000_000),
            "1700000000_17d748c166afd876ceb872a291e5befdca771528"
        );
    }

    #[test]
    fn cookie_jar_prefers_secure_sapisid() {
        let jar =
            CookieJar::from_header("SAPISID=test-sapisid; __Secure-3PAPISID=test-secure-sapisid");
        assert_eq!(jar.sapisid(), Some(Secret::new("test-secure-sapisid")));
    }

    #[test]
    fn cookie_jar_accepts_plain_sapisid() {
        let jar = CookieJar::from_header("SAPISID=test-sapisid");
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn cookie_jar_accepts_secure_sapisid() {
        let jar = CookieJar::from_header("__Secure-3PAPISID=test-secure-sapisid");
        assert_eq!(jar.sapisid(), Some(Secret::new("test-secure-sapisid")));
    }

    #[test]
    fn cookie_jar_requires_exact_names() {
        let jar =
            CookieJar::from_header("__Secure-1PAPISID=test-secure-sapisid; XSAPISID=test-sapisid");
        assert_eq!(jar.sapisid(), None);
    }

    #[test]
    fn cookie_jar_preserves_equals_in_value() {
        assert_eq!(
            CookieJar::from_header("SAPISID=ab=c").sapisid(),
            Some(Secret::new("ab=c"))
        );
    }

    #[test]
    fn cookie_jar_skips_empty_and_malformed_pairs() {
        assert_eq!(CookieJar::from_header("").sapisid(), None);
        assert_eq!(CookieJar::from_header("SAPISID").sapisid(), None);
        let jar = CookieJar::from_header("__Secure-3PAPISID; SAPISID=test-sapisid");
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn cookie_jar_iterates_pairs_and_preserves_equals_in_values() {
        let jar = CookieJar::from_header("SID=test-cookie; malformed; SAPISID=test=auth");
        assert_eq!(
            jar.pairs().collect::<Vec<_>>(),
            [("SID", "test-cookie"), ("SAPISID", "test=auth")]
        );
    }

    #[test]
    fn cookie_jar_debug_is_redacted() {
        let jar = CookieJar::from_header("SAPISID=test-sapisid");
        assert!(!format!("{jar:?}").contains("test-sapisid"));
    }

    #[test]
    fn from_pairs_builds_header() {
        let jar = CookieJar::from_pairs([("SAPISID", "test-sapisid"), ("HSID", "test-cookie")]);
        assert_eq!(
            jar.header().expose(),
            "SAPISID=test-sapisid; HSID=test-cookie"
        );
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn from_pairs_skips_unrepresentable_pairs() {
        let jar = CookieJar::from_pairs([
            ("", "test-cookie"),
            ("A=B", "test-cookie"),
            ("A;B", "test-cookie"),
            ("BAD", "x;y"),
            (" SAPISID ", " test-sapisid "),
        ]);
        assert_eq!(jar.header().expose(), "SAPISID=test-sapisid");
    }

    #[test]
    fn from_pairs_accepts_owned_strings() {
        let pairs = vec![(String::from("SID"), String::from("test-cookie"))];
        assert_eq!(
            CookieJar::from_pairs(pairs).header().expose(),
            "SID=test-cookie"
        );
    }

    #[test]
    fn shared_cookies_clones_share_one_slot() {
        let shared = SharedCookies::default();
        let clone = shared.clone();
        clone.set(Some(CookieJar::from_header("SAPISID=test-sapisid")));
        assert_eq!(
            shared.get().and_then(|jar| jar.sapisid()),
            Some(Secret::new("test-sapisid"))
        );
        shared.set(None);
        assert!(clone.get().is_none());
    }

    #[test]
    fn shared_cookies_debug_hides_values() {
        let shared = SharedCookies::new(Some(CookieJar::from_header("SAPISID=test-sapisid")));
        assert_eq!(format!("{shared:?}"), "SharedCookies(set)");
        assert_eq!(
            format!("{:?}", SharedCookies::default()),
            "SharedCookies(empty)"
        );
    }
}
