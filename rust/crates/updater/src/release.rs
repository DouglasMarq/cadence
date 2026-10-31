use semver::Version;
use serde::Deserialize;

use crate::error::{Result, UpdateError};
use crate::platform::Platform;

const CHECKSUMS_ASSET: &str = "SHA256SUMS";
const TAG_PREFIX: char = 'v';

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub html_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: Version,
    pub page_url: String,
    pub asset: ReleaseAsset,
    pub checksums: ReleaseAsset,
}

impl Release {
    pub fn offer(&self, current: &Version, platform: Platform) -> Result<Option<Offer>> {
        let version = self.version()?;
        if version <= *current {
            tracing::debug!("the latest release {version} is not newer than {current}");
            return Ok(None);
        }
        let name = platform.asset_name(&version);
        let (Some(asset), Some(checksums)) = (self.asset(&name), self.asset(CHECKSUMS_ASSET))
        else {
            tracing::debug!("release {version} has no {name} with checksums");
            return Ok(None);
        };
        Ok(Some(Offer {
            page_url: self.html_url.clone(),
            asset: asset.clone(),
            checksums: checksums.clone(),
            version,
        }))
    }

    fn version(&self) -> Result<Version> {
        let tag = self.tag_name.as_str();
        Version::parse(tag.strip_prefix(TAG_PREFIX).unwrap_or(tag)).map_err(|source| {
            UpdateError::InvalidVersion {
                tag: tag.to_owned(),
                source,
            }
        })
    }

    fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: Platform = Platform::Linux("x86_64");
    const INSTALLED: Version = Version::new(1, 0, 0);

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_owned(),
            browser_download_url: format!("https://downloads.example/{name}"),
            size: 7,
        }
    }

    fn release(tag: &str, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            html_url: format!("https://releases.example/{tag}"),
            assets: assets.iter().map(|name| asset(name)).collect(),
        }
    }

    fn offer_of(tag: &str) -> Option<Offer> {
        let name = format!(
            "Cadence-{}-x86_64.AppImage",
            tag.trim_start_matches(TAG_PREFIX)
        );
        release(tag, &[&name, CHECKSUMS_ASSET])
            .offer(&INSTALLED, LINUX)
            .unwrap()
    }

    #[test]
    fn given_a_newer_tag_when_comparing_then_it_is_offered_with_its_assets() {
        let offer = offer_of("v1.0.1").unwrap();
        assert_eq!(offer.version, Version::new(1, 0, 1));
        assert_eq!(offer.page_url, "https://releases.example/v1.0.1");
        assert_eq!(offer.asset.name, "Cadence-1.0.1-x86_64.AppImage");
        assert_eq!(offer.checksums.name, CHECKSUMS_ASSET);
    }

    #[test]
    fn given_a_tag_without_the_prefix_when_comparing_then_it_is_still_a_version() {
        assert!(offer_of("1.0.1").is_some());
    }

    #[test]
    fn given_the_installed_or_an_older_tag_when_comparing_then_nothing_is_offered() {
        assert_eq!(offer_of("v1.0.0"), None);
        assert_eq!(offer_of("v0.9.9"), None);
    }

    #[test]
    fn given_double_digit_parts_when_comparing_then_they_are_ordered_as_numbers() {
        let installed = Version::new(1, 9, 0);
        let newer = release("v1.10.0", &["Cadence-1.10.0-x86_64.AppImage", "SHA256SUMS"]);
        assert!(newer.offer(&installed, LINUX).unwrap().is_some());
    }

    #[test]
    fn given_a_prerelease_of_the_installed_version_when_comparing_then_it_is_older() {
        assert_eq!(offer_of("v1.0.0-rc.1"), None);
    }

    #[test]
    fn given_no_asset_for_this_platform_when_comparing_then_nothing_is_offered() {
        let other = release("v1.0.1", &["Cadence-1.0.1-macos-arm64.zip", "SHA256SUMS"]);
        assert_eq!(other.offer(&INSTALLED, LINUX).unwrap(), None);
    }

    #[test]
    fn given_no_checksums_when_comparing_then_nothing_is_offered() {
        let unverifiable = release("v1.0.1", &["Cadence-1.0.1-x86_64.AppImage"]);
        assert_eq!(unverifiable.offer(&INSTALLED, LINUX).unwrap(), None);
    }

    #[test]
    fn given_a_tag_that_is_no_version_when_comparing_then_it_is_an_error() {
        let error = release("nightly", &[])
            .offer(&INSTALLED, LINUX)
            .unwrap_err();
        assert!(matches!(error, UpdateError::InvalidVersion { .. }));
        assert!(error.to_string().contains("nightly"), "{error}");
    }
}
