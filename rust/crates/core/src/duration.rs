pub const MILLISECONDS_PER_SECOND: u64 = 1000;
pub const MILLISECONDS_PER_MINUTE: u64 = 60 * MILLISECONDS_PER_SECOND;
pub const MILLISECONDS_PER_CENTISECOND: u64 = MILLISECONDS_PER_SECOND / 100;

pub fn seconds_to_ms(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * MILLISECONDS_PER_SECOND as f64).round() as u64
}

pub fn ms_to_seconds(milliseconds: u64) -> f64 {
    milliseconds as f64 / MILLISECONDS_PER_SECOND as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_fractional_seconds_when_converting_then_milliseconds_are_rounded() {
        assert_eq!(seconds_to_ms(201.5), 201_500);
        assert_eq!(seconds_to_ms(0.0004), 0);
        assert_eq!(seconds_to_ms(0.0006), 1);
    }

    #[test]
    fn given_a_negative_or_non_finite_duration_when_converting_then_it_is_zero() {
        assert_eq!(seconds_to_ms(-3.0), 0);
        assert_eq!(seconds_to_ms(f64::NAN), 0);
        assert_eq!(seconds_to_ms(f64::INFINITY), 0);
    }

    #[test]
    fn given_milliseconds_when_converting_then_fractional_seconds_are_kept() {
        assert_eq!(ms_to_seconds(201_500), 201.5);
        assert_eq!(ms_to_seconds(1), 0.001);
        assert_eq!(ms_to_seconds(0), 0.0);
    }
}
