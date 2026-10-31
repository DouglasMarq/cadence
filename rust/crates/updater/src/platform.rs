use std::env::consts::ARCH;

use semver::Version;

const RUST_ARM_ARCHITECTURE: &str = "aarch64";
const MACOS_ARM_ARCHITECTURE: &str = "arm64";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs(&'static str),
    Linux(&'static str),
}

impl Platform {
    pub fn current() -> Self {
        match cfg!(target_os = "macos") {
            true => Self::MacOs(ARCH),
            false => Self::Linux(ARCH),
        }
    }

    pub fn asset_name(self, version: &Version) -> String {
        match self {
            Self::MacOs(arch) => format!("Cadence-{version}-macos-{}.zip", macos_machine(arch)),
            Self::Linux(arch) => format!("Cadence-{version}-{arch}.AppImage"),
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
}
