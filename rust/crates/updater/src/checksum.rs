use sha2::{Digest, Sha256};

const SHA256_HEX_LENGTH: usize = 64;
const BINARY_MODE_MARKER: char = '*';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(text: &str) -> Option<Self> {
        let is_digest =
            text.len() == SHA256_HEX_LENGTH && text.chars().all(|digit| digit.is_ascii_hexdigit());
        is_digest.then(|| Self(text.to_ascii_lowercase()))
    }

    pub fn finish(hasher: Sha256) -> Self {
        Self(
            hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    }
}

fn parsed_line(line: &str) -> Option<(Sha256Digest, &str)> {
    let (digest, name) = line.trim().split_once(char::is_whitespace)?;
    let name = name.trim_start();
    let name = name.strip_prefix(BINARY_MODE_MARKER).unwrap_or(name);
    Some((Sha256Digest::parse(digest)?, name))
}

pub fn listed_checksum(checksums: &str, asset: &str) -> Option<Sha256Digest> {
    checksums
        .lines()
        .filter_map(parsed_line)
        .find_map(|(digest, name)| (name == asset).then_some(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn digest() -> Sha256Digest {
        Sha256Digest::parse(DIGEST).unwrap()
    }

    #[test]
    fn given_a_sha256sum_listing_when_looking_up_an_asset_then_its_digest_is_found() {
        let listing = format!("{DIGEST}  Cadence-1.0.0-x86_64.AppImage\n");
        assert_eq!(
            listed_checksum(&listing, "Cadence-1.0.0-x86_64.AppImage"),
            Some(digest())
        );
    }

    #[test]
    fn given_a_binary_mode_listing_when_looking_up_an_asset_then_the_marker_is_skipped() {
        let listing = format!("{DIGEST} *Cadence-1.0.0-macos-arm64.zip\n");
        assert_eq!(
            listed_checksum(&listing, "Cadence-1.0.0-macos-arm64.zip"),
            Some(digest())
        );
    }

    #[test]
    fn given_an_uppercase_digest_when_it_is_read_then_it_equals_the_lowercase_one() {
        let listing = format!("{}  asset\n", DIGEST.to_uppercase());
        assert_eq!(listed_checksum(&listing, "asset"), Some(digest()));
    }

    #[test]
    fn given_a_listing_without_the_asset_when_looking_it_up_then_nothing_is_found() {
        let listing = format!("{DIGEST}  other-asset\n");
        assert_eq!(listed_checksum(&listing, "asset"), None);
    }

    #[test]
    fn given_a_name_that_only_starts_like_the_asset_when_looking_it_up_then_it_is_not_matched() {
        let listing = format!("{DIGEST}  asset.sig\n");
        assert_eq!(listed_checksum(&listing, "asset"), None);
    }

    #[test]
    fn given_malformed_lines_when_looking_up_an_asset_then_they_are_skipped() {
        let listing = format!("not a digest  asset\nasset\n\n{DIGEST}  asset\n");
        assert_eq!(listed_checksum(&listing, "asset"), Some(digest()));
    }

    #[test]
    fn given_text_that_is_not_a_digest_when_parsing_then_it_is_refused() {
        assert_eq!(Sha256Digest::parse("abc"), None);
        assert_eq!(Sha256Digest::parse(&"z".repeat(SHA256_HEX_LENGTH)), None);
    }

    #[test]
    fn given_known_input_when_hashing_then_the_digest_is_the_published_one() {
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        assert_eq!(Sha256Digest::finish(hasher), digest());
    }
}
