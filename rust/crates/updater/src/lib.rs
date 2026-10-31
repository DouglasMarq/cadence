mod checksum;
mod download;
mod error;
mod feed;
mod installer;
mod platform;
mod relaunch;
mod release;
mod updater;

pub use error::{Result, UpdateError};
pub use feed::{ReleaseFeed, USER_AGENT, https_only_client};
pub use installer::{AppImage, Installer, Launch, MacBundle, STAGING_DIRECTORY, installer_for};
pub use platform::Platform;
pub use relaunch::Relaunch;
pub use release::{Offer, Release, ReleaseAsset};
pub use semver::Version;
pub use updater::{Progress, Updater};
