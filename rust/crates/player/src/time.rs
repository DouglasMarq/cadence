use cadence_core::duration::MILLISECONDS_PER_SECOND;

pub(crate) fn mpv_seconds_from_milliseconds(milliseconds: u64) -> String {
    format!(
        "{}.{:03}",
        milliseconds / MILLISECONDS_PER_SECOND,
        milliseconds % MILLISECONDS_PER_SECOND
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_milliseconds_as_mpv_seconds() {
        assert_eq!(mpv_seconds_from_milliseconds(12_345), "12.345");
        assert_eq!(mpv_seconds_from_milliseconds(4_005), "4.005");
        assert_eq!(mpv_seconds_from_milliseconds(0), "0.000");
    }
}
