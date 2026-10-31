use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use semver::Version;

use super::{Installer, Launch, writable_staging_beside};
use crate::error::{Result, io_error};
use crate::relaunch::Relaunch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppImage {
    image: PathBuf,
    staging: PathBuf,
}

impl AppImage {
    pub fn detect(launch: &Launch) -> Option<Arc<dyn Installer>> {
        let image = launch.app_image.clone()?;
        let is_running_from_it = launch.executable.starts_with(launch.app_dir.as_ref()?);
        if !is_running_from_it {
            tracing::debug!("APPIMAGE was inherited from another AppImage, ignoring it");
            return None;
        }
        let staging = writable_staging_beside(&image)?;
        Some(Arc::new(Self { image, staging }))
    }
}

impl Installer for AppImage {
    fn staging_directory(&self) -> &Path {
        &self.staging
    }

    fn install(&self, download: &Path, _version: &Version) -> Result<()> {
        let permissions = fs::metadata(&self.image)
            .map_err(io_error("reading the installed AppImage"))?
            .permissions();
        fs::set_permissions(download, permissions)
            .map_err(io_error("preparing the new AppImage"))?;
        fs::rename(download, &self.image).map_err(io_error("replacing the AppImage"))
    }

    fn relaunch(&self) -> Relaunch {
        Relaunch::new(&self.image, [])
    }
}
