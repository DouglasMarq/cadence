use crate::duration::{
    MILLISECONDS_PER_CENTISECOND, MILLISECONDS_PER_MINUTE, MILLISECONDS_PER_SECOND,
};

pub fn lrc_timestamp(milliseconds: u64) -> String {
    let minutes = milliseconds / MILLISECONDS_PER_MINUTE;
    let seconds = (milliseconds % MILLISECONDS_PER_MINUTE) / MILLISECONDS_PER_SECOND;
    let centiseconds = (milliseconds % MILLISECONDS_PER_SECOND) / MILLISECONDS_PER_CENTISECOND;
    format!("[{minutes:02}:{seconds:02}.{centiseconds:02}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_position_when_formatting_then_it_is_minutes_seconds_and_centiseconds() {
        assert_eq!(lrc_timestamp(0), "[00:00.00]");
        assert_eq!(lrc_timestamp(61_234), "[01:01.23]");
        assert_eq!(lrc_timestamp(3_600_000), "[60:00.00]");
    }
}
