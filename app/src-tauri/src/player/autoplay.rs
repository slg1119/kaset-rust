//! Single-flight radio scheduling, independent of Tauri and network I/O.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ticket {
    pub generation: u64,
    pub epoch: u64,
}

#[derive(Default)]
pub(super) struct Autoplay {
    in_flight: bool,
    pending: bool,
}

impl Autoplay {
    pub fn request(&mut self) {
        self.pending = true;
    }

    pub fn cancel_pending(&mut self) {
        self.pending = false;
    }

    pub fn start<T>(&mut self, seed: impl FnOnce() -> Option<T>) -> Option<T> {
        if self.in_flight || !std::mem::take(&mut self.pending) {
            return None;
        }
        let seed = seed()?;
        self.in_flight = true;
        Some(seed)
    }

    pub fn finish(&mut self, ticket: Ticket, current: Ticket) -> bool {
        self.in_flight = false;
        ticket == current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_pending_requests_and_rejects_stale_pages() {
        let mut autoplay = Autoplay::default();
        let first = Ticket {
            generation: 0,
            epoch: 0,
        };
        let edited = Ticket {
            generation: 1,
            ..first
        };
        let signed_out = Ticket { epoch: 1, ..edited };

        assert_eq!(autoplay.start(|| Some("first")), None);
        autoplay.request();
        assert_eq!(autoplay.start(|| Some("first")), Some("first"));
        autoplay.request();
        autoplay.request();
        assert_eq!(autoplay.start(|| Some("next")), None);
        assert!(!autoplay.finish(first, edited));
        assert_eq!(autoplay.start(|| Some("next")), Some("next"));

        autoplay.request();
        autoplay.cancel_pending();
        assert!(!autoplay.finish(edited, signed_out));
        assert_eq!(autoplay.start(|| Some("next")), None);
        autoplay.request();
        assert_eq!(autoplay.start(|| Some("retry")), Some("retry"));
        assert!(autoplay.finish(signed_out, signed_out));
        assert_eq!(autoplay.start(|| Some("retry")), None);
    }
}
