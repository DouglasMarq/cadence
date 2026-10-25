const MAX_RATING: u32 = 5;
const PARTIAL_ISO_DATE_SHAPE: &[u8] = b"dddd-dd-dd";
const DIGIT_PLACEHOLDER: u8 = b'd';
const YEAR_LENGTH: usize = 4;
const YEAR_MONTH_LENGTH: usize = 7;
const FULL_DATE_LENGTH: usize = 10;
const UNSET_PLAY_DATE_MARKER: &str = "0001-";

pub(crate) fn rating(value: Option<u32>) -> Option<u8> {
    value
        .filter(|rating| *rating > 0)
        .map(|rating| rating.min(MAX_RATING) as u8)
}

pub(crate) fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

pub(crate) fn partial_iso_year(input: Option<&str>) -> Option<i32> {
    let trimmed = input?.trim();
    if !matches_partial_iso_date(trimmed.as_bytes()) {
        return None;
    }
    trimmed.get(0..YEAR_LENGTH)?.parse().ok()
}

fn matches_partial_iso_date(bytes: &[u8]) -> bool {
    matches!(
        bytes.len(),
        YEAR_LENGTH | YEAR_MONTH_LENGTH | FULL_DATE_LENGTH
    ) && bytes
        .iter()
        .zip(PARTIAL_ISO_DATE_SHAPE)
        .all(|(byte, expected)| match *expected {
            DIGIT_PLACEHOLDER => byte.is_ascii_digit(),
            separator => *byte == separator,
        })
}

pub(crate) fn play_date(value: Option<String>) -> Option<String> {
    value.filter(|date| !date.is_empty() && !date.contains(UNSET_PLAY_DATE_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_iso_dates() {
        assert_eq!(partial_iso_year(Some("2001")), Some(2001));
        assert_eq!(partial_iso_year(Some("2001-02")), Some(2001));
        assert_eq!(partial_iso_year(Some("2001-02-03")), Some(2001));
        assert_eq!(partial_iso_year(Some("2001-02-03T00:00:00Z")), None);
        assert_eq!(partial_iso_year(Some("")), None);
        assert_eq!(partial_iso_year(None), None);
    }
}
