use std::fmt;

/// A string that must never appear in logs. `Debug` prints `Secret(REDACTED)`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let secret = Secret::new("test-cookie");
        assert_eq!(format!("{secret:?}"), "Secret(REDACTED)");
        assert_eq!(secret.expose(), "test-cookie");
    }
}
