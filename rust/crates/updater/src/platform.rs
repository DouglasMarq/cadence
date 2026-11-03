use std::env::consts::{ARCH, OS};

use semver::Version;

const RUST_ARM_ARCHITECTURE: &str = "aarch64";
const MACOS_ARM_ARCHITECTURE: &str = "arm64";
const MACOS: &str = "macos";
const WINDOWS: &str = "windows";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs(&'static str),
    Linux(&'static str),
    Windows(&'static str),
}

impl Platform {
    pub fn current() -> Self {
        match OS {
            MACOS => Self::MacOs(ARCH),
            WINDOWS => Self::Windows(ARCH),
            _ => Self::Linux(ARCH),
        }
    }

    pub fn asset_name(self, version: &Version) -> String {
        match self {
            Self::MacOs(arch) => format!("Cadence-{version}-macos-{}.zip", macos_machine(arch)),
            Self::Linux(arch) => format!("Cadence-{version}-{arch}.AppImage"),
            Self::Windows(arch) => format!("Cadence-{version}-windows-{arch}.zip"),
        }
    }
}

fn macos_machine(arch: &str) -> &str {
    match arch {
        RUST_ARM_ARCHITECTURE => MACOS_ARM_ARCHITECTURE,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERSION: Version = Version::new(1, 2, 3);

    #[test]
    fn given_apple_silicon_when_naming_the_asset_then_it_uses_the_name_of_build_app() {
        assert_eq!(
            Platform::MacOs("aarch64").asset_name(&VERSION),
            "Cadence-1.2.3-macos-arm64.zip"
        );
    }

    #[test]
    fn given_an_intel_mac_when_naming_the_asset_then_the_architecture_is_kept() {
        assert_eq!(
            Platform::MacOs("x86_64").asset_name(&VERSION),
            "Cadence-1.2.3-macos-x86_64.zip"
        );
    }

    #[test]
    fn given_linux_when_naming_the_asset_then_it_uses_the_name_of_build_appimage() {
        assert_eq!(
            Platform::Linux("x86_64").asset_name(&VERSION),
            "Cadence-1.2.3-x86_64.AppImage"
        );
    }

    #[test]
    fn given_windows_when_naming_the_asset_then_it_uses_the_name_of_the_portable_zip() {
        assert_eq!(
            Platform::Windows("x86_64").asset_name(&VERSION),
            "Cadence-1.2.3-windows-x86_64.zip"
        );
    }

    #[cfg(windows)]
    #[test]
    fn given_a_windows_build_when_naming_its_platform_then_it_is_windows() {
        assert_eq!(Platform::current(), Platform::Windows(ARCH));
    }
}
