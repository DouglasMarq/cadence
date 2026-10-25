use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backoff::exponential_delay;

const FIRST_RETRY_DELAY: Duration = Duration::from_secs(2);
const LONGEST_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy)]
struct PageFailure {
    failures: u32,
    retry_at: Instant,
}

#[derive(Debug, Default)]
pub struct PageBackoff {
    failed: HashMap<usize, PageFailure>,
}

impl PageBackoff {
    pub fn record_failure(&mut self, page: usize, now: Instant) {
        let failures = self
            .failed
            .get(&page)
            .map_or(1, |failure| failure.failures.saturating_add(1));
        let delay = retry_delay(failures);
        let retry_at = now.checked_add(delay).unwrap_or(now);
        self.failed.insert(page, PageFailure { failures, retry_at });
    }

    pub fn delay_of(&self, page: usize) -> Option<Duration> {
        self.failed
            .get(&page)
            .map(|failure| retry_delay(failure.failures))
    }

    pub fn allows(&self, page: usize, now: Instant) -> bool {
        self.failed
            .get(&page)
            .is_none_or(|failure| now >= failure.retry_at)
    }

    pub fn succeeded(&mut self, page: usize) {
        self.failed.remove(&page);
    }

    pub fn clear(&mut self) {
        self.failed.clear();
    }
}

fn retry_delay(failures: u32) -> Duration {
    exponential_delay(FIRST_RETRY_DELAY, failures, LONGEST_RETRY_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_doubles_the_wait_up_to_a_minute() {
        let mut backoff = PageBackoff::default();
        let now = Instant::now();
        let delays: Vec<u64> = (0..8)
            .filter_map(|_| {
                backoff.record_failure(3, now);
                backoff.delay_of(3)
            })
            .map(|delay| delay.as_secs())
            .collect();
        assert_eq!(delays, [2, 4, 8, 16, 32, 60, 60, 60]);
    }

    #[test]
    fn a_failed_page_waits_until_its_retry_time_and_success_forgets_it() {
        let mut backoff = PageBackoff::default();
        let now = Instant::now();
        backoff.record_failure(3, now);
        assert!(!backoff.allows(3, now));
        assert!(!backoff.allows(3, now + Duration::from_secs(1)));
        assert!(backoff.allows(3, now + FIRST_RETRY_DELAY));
        assert!(backoff.allows(4, now));
        backoff.succeeded(3);
        assert!(backoff.allows(3, now));
        backoff.record_failure(3, now);
        backoff.record_failure(3, now);
        assert_eq!(backoff.delay_of(3), Some(Duration::from_secs(4)));
        backoff.clear();
        assert!(backoff.allows(3, now));
    }
}
