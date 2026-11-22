use crate::secret::Secret;

use sha1::{Digest, Sha1};

/// SAPISIDHASH value for an InnerTube request. The origin is part of the hash,
/// so a value computed for music.youtube.com is not valid for www.youtube.com.
pub fn sapisid_hash(sapisid: &str, origin: &str, timestamp: u64) -> String {
    let digest = Sha1::digest(format!("{timestamp} {sapisid} {origin}").as_bytes());
    format!("{timestamp}_{}", hex::encode(digest))
}

/// A `Cookie` header for youtube.com. Values stay inside `Secret`.
#[derive(Debug, Clone)]
pub struct CookieJar {
    header: Secret,
}

impl CookieJar {
    pub fn from_header(header: &str) -> Self {
        Self {
            header: Secret::new(header.trim()),
        }
    }

    pub fn header(&self) -> &Secret {
        &self.header
    }

    pub fn sapisid(&self) -> Option<Secret> {
        self.header
            .expose()
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(name, _)| *name == "SAPISID")
            .map(|(_, value)| Secret::new(value))
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
    fn cookie_jar_finds_exact_sapisid() {
        let jar = CookieJar::from_header("__Secure-3PAPISID=other; SAPISID=test-sapisid; HSID=x");
        assert_eq!(jar.sapisid(), Some(Secret::new("test-sapisid")));
    }

    #[test]
    fn cookie_jar_without_sapisid() {
        assert_eq!(CookieJar::from_header("HSID=x").sapisid(), None);
    }

    #[test]
    fn cookie_jar_debug_is_redacted() {
        let jar = CookieJar::from_header("SAPISID=test-sapisid");
        assert!(!format!("{jar:?}").contains("test-sapisid"));
    }
}
