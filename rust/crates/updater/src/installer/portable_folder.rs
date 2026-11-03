use std::fs;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use semver::Version;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use super::swap::{Swap, put_all_in_place};
use super::{Installer, Launch, writable_staging_beside};
use crate::error::{Result, UpdateError, io_error};
use crate::relaunch::Relaunch;

const TAR: &str = "tar.exe";
const EXECUTABLE_NAME: &str = "cadence.exe";
const PLAYER_LIBRARY: &str = "libmpv-2.dll";
const EXTRACTED_DIRECTORY: &str = "extracted";
const PREVIOUS_DIRECTORY: &str = "previous";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableFolder {
    executable: PathBuf,
    library: PathBuf,
    staging: PathBuf,
}

impl PortableFolder {
    pub fn detect(launch: &Launch) -> Option<Arc<dyn Installer>> {
        let library = launch.executable.parent()?.join(PLAYER_LIBRARY);
        if !library.is_file() {
            return None;
        }
        let staging = writable_staging_beside(&launch.executable)?;
        Some(Arc::new(Self {
            executable: launch.executable.clone(),
            library,
            staging,
        }))
    }

    fn swaps(&self) -> [Swap; 2] {
        let swap_of = |installed: &Path, name: &str| Swap {
            installed: installed.to_owned(),
            staged: self.staging.join(EXTRACTED_DIRECTORY).join(name),
            backup: self.staging.join(PREVIOUS_DIRECTORY).join(name),
        };
        [
            swap_of(&self.library, PLAYER_LIBRARY),
            swap_of(&self.executable, EXECUTABLE_NAME),
        ]
    }
}

impl Installer for PortableFolder {
    fn staging_directory(&self) -> &Path {
        &self.staging
    }

    fn install(&self, download: &Path, _version: &Version) -> Result<()> {
        let extracted = self.staging.join(EXTRACTED_DIRECTORY);
        fs::create_dir_all(&extracted)
            .and_then(|()| fs::create_dir_all(self.staging.join(PREVIOUS_DIRECTORY)))
            .map_err(io_error("creating the update folders"))?;
        extract_zip(download, &extracted)?;
        let swaps = self.swaps();
        validate_release(&swaps)?;
        put_all_in_place(&swaps, |from, to| fs::rename(from, to))
    }

    fn relaunch(&self) -> Relaunch {
        Relaunch::new(&self.executable, [])
    }
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let status = Command::new(TAR)
        .arg("-x")
        .arg("-f")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(io_error("starting tar"))?;
    match status.success() {
        true => Ok(()),
        false => Err(UpdateError::Extract(format!("tar exited with {status}"))),
    }
}

fn validate_release(swaps: &[Swap]) -> Result<()> {
    match swaps.iter().find(|swap| !swap.staged.is_file()) {
        Some(missing) => Err(UpdateError::InvalidBundle(format!(
            "{} is missing",
            missing.staged.display()
        ))),
        None => Ok(()),
    }
}
