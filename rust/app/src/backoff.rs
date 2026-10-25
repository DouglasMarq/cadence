use std::time::Duration;

const GROWTH_FACTOR: u32 = 2;

pub fn exponential_delay(first: Duration, failures: u32, longest: Duration) -> Duration {
    let doublings = failures.saturating_sub(1);
    first
        .saturating_mul(GROWTH_FACTOR.saturating_pow(doublings))
        .min(longest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: Duration = Duration::from_secs(2);
    const LONGEST: Duration = Duration::from_secs(60);

    #[test]
    fn given_repeated_failures_when_computing_the_delay_then_it_doubles_up_to_the_longest() {
        let delays: Vec<u64> = (0..=7)
            .map(|failures| exponential_delay(FIRST, failures, LONGEST).as_secs())
            .collect();
        assert_eq!(delays, [2, 2, 4, 8, 16, 32, 60, 60]);
    }

    #[test]
    fn given_a_huge_failure_count_when_computing_the_delay_then_it_saturates_at_the_longest() {
        assert_eq!(exponential_delay(FIRST, u32::MAX, LONGEST), LONGEST);
        assert_eq!(
            exponential_delay(Duration::MAX, 2, Duration::MAX),
            Duration::MAX
        );
    }
}
