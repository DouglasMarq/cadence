const WEB_SCHEMES: [&str; 2] = ["http://", "https://"];

pub fn is_web_url(url: &str) -> bool {
    let lowercase = url.to_ascii_lowercase();
    WEB_SCHEMES
        .iter()
        .any(|scheme| lowercase.len() > scheme.len() && lowercase.starts_with(scheme))
}

pub fn trimmed_base_url(url: &str) -> &str {
    url.trim().trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_web_links_in_any_case_when_checking_then_they_are_web_urls() {
        assert!(is_web_url("https://example.com/stream"));
        assert!(is_web_url("http://example.com"));
        assert!(is_web_url("HTTPS://EXAMPLE.COM"));
    }

    #[test]
    fn given_other_schemes_or_a_bare_scheme_when_checking_then_they_are_not_web_urls() {
        assert!(!is_web_url("https://"));
        assert!(!is_web_url("ftp://example.com"));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("example.com"));
        assert!(!is_web_url(""));
    }

    #[test]
    fn given_spaces_and_trailing_slashes_when_trimming_a_base_url_then_both_are_removed() {
        assert_eq!(
            trimmed_base_url("  https://music.example.com/app// "),
            "https://music.example.com/app"
        );
        assert_eq!(
            trimmed_base_url("https://example.com"),
            "https://example.com"
        );
        assert_eq!(trimmed_base_url(" / "), "");
    }
}
