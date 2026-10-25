use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn unix_now_seconds() -> u64 {
    since_unix_epoch().as_secs()
}

pub fn unix_now_ms() -> i64 {
    i64::try_from(since_unix_epoch().as_millis()).unwrap_or(i64::MAX)
}

pub fn since_unix_epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MILLISECONDS_PER_SECOND: i64 = 1_000;
    const SECONDS_IN_2020: u64 = 1_577_836_800;

    #[test]
    fn given_the_system_clock_when_reading_unix_time_then_seconds_and_milliseconds_agree() {
        let seconds = unix_now_seconds();
        let milliseconds = unix_now_ms();
        assert!(seconds > SECONDS_IN_2020);
        let seconds_from_milliseconds =
            u64::try_from(milliseconds / MILLISECONDS_PER_SECOND).unwrap();
        assert!(seconds_from_milliseconds.abs_diff(seconds) <= 1);
        assert!(since_unix_epoch().as_secs().abs_diff(seconds) <= 1);
    }
}
