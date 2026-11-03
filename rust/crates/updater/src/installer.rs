mod app_image;
#[cfg(unix)]
mod mac_bundle;
#[cfg(windows)]
mod portable_folder;
mod swap;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use semver::Version;

pub use app_image::AppImage;
#[cfg(unix)]
pub use mac_bundle::MacBundle;
#[cfg(windows)]
pub use portable_folder::PortableFolder;

use crate::error::Result;
use crate::relaunch::Relaunch;

pub const STAGING_DIRECTORY: &str = ".cadence-update";

const APP_IMAGE_VARIABLE: &str = "APPIMAGE";
const APP_DIR_VARIABLE: &str = "APPDIR";

type Detector = fn(&Launch) -> Option<Arc<dyn Installer>>;

#[cfg(unix)]
const DETECTORS: [Detector; 2] = [MacBundle::detect, AppImage::detect];
#[cfg(windows)]
const DETECTORS: [Detector; 1] = [PortableFolder::detect];

pub trait Installer: Send + Sync {
    fn staging_directory(&self) -> &Path;
    fn install(&self, download: &Path, version: &Version) -> Result<()>;
    fn relaunch(&self) -> Relaunch;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launch {
    pub executable: PathBuf,
    pub app_image: Option<PathBuf>,
    pub app_dir: Option<PathBuf>,
}

impl Launch {
    pub fn from_env() -> io::Result<Self> {
        Ok(Self {
            executable: running_executable()?,
            app_image: path_variable(APP_IMAGE_VARIABLE),
            app_dir: path_variable(APP_DIR_VARIABLE).and_then(|path| path.canonicalize().ok()),
        })
    }
}

#[cfg(unix)]
fn running_executable() -> io::Result<PathBuf> {
    std::env::current_exe()?.canonicalize()
}

#[cfg(windows)]
fn running_executable() -> io::Result<PathBuf> {
    std::env::current_exe()
}

fn path_variable(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn installer_for(launch: &Launch) -> Option<Arc<dyn Installer>> {
    DETECTORS.iter().find_map(|detect| detect(launch))
}

fn writable_staging_beside(installed: &Path) -> Option<PathBuf> {
    let staging = installed.parent()?.join(STAGING_DIRECTORY);
    probe(&staging)
        .inspect_err(|error| {
            tracing::debug!("{} cannot be written: {error}", staging.display());
        })
        .ok()?;
    Some(staging)
}

fn probe(staging: &Path) -> io::Result<()> {
    fs::create_dir_all(staging)?;
    fs::remove_dir_all(staging)
}
