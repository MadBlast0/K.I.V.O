//! Event timestamps: a monotonic reading for measuring latency (plan §97) plus wall-clock time
//! for display and storage.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// When something happened.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timestamp {
    /// Microseconds since this process started. Monotonic: use it for latency, never for display.
    pub mono_us: u64,
    /// Milliseconds since the Unix epoch (UTC). For display and storage.
    pub wall_ms: i64,
}

impl Timestamp {
    pub fn now() -> Self {
        static START: OnceLock<Instant> = OnceLock::new();
        let start = *START.get_or_init(Instant::now);
        let mono_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
        let wall_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        Self { mono_us, wall_ms }
    }

    /// Microseconds from `earlier` to `self` on the monotonic clock (0 if `earlier` is later).
    pub fn micros_since(&self, earlier: &Timestamp) -> u64 {
        self.mono_us.saturating_sub(earlier.mono_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_part_never_goes_backwards() {
        let a = Timestamp::now();
        let b = Timestamp::now();
        assert!(b.mono_us >= a.mono_us);
        assert_eq!(a.micros_since(&b), 0, "saturates instead of underflowing");
    }

    #[test]
    fn wall_clock_is_after_2026() {
        assert!(Timestamp::now().wall_ms > 1_767_225_600_000);
    }
}
