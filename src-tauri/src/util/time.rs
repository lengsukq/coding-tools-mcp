use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix timestamp in whole seconds.
pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Current Unix timestamp in whole milliseconds.
pub fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_timestamps_are_monotonic_and_non_zero() {
        let secs = unix_now_secs();
        let ms = unix_now_ms();
        assert!(secs > 1_700_000_000);
        assert!(ms >= secs * 1_000);
    }
}
