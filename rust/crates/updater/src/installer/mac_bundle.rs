use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use semver::Version;

use super::{Installer, Launch, writable_staging_beside};
use crate::error::{Result, UpdateError, io_error};
use crate::relaunch::Relaunch;

const DITTO: &str = "/usr/bin/ditto";
const OPEN: &str = "/usr/bin/open";
const BUNDLE_NAME: &str = "Cadence.app";
const BUNDLE_EXTENSION: &str = "app";
const CONTENTS_DIRECTORY: &str = "Contents";
const EXECUTABLES_DIRECTORY: &str = "MacOS";
const EXECUTABLE_NAME: &str = "cadence";
const INFO_PLIST: &str = "Info.plist";
const VERSION_KEY: &str = "<key>CFBundleShortVersionString</key>";
const VALUE_START: &str = "<string>";
const VALUE_END: &str = "</string>";
const EXTRACTED_DIRECTORY: &str = "extracted";
const PREVIOUS_BUNDLE: &str = "previous.app";
const EXECUTE_BITS: u32 = 0o111;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacBundle {
    bundle: PathBuf,
    staging: PathBuf,
}

struct BundleSwap<'a> {
    installed: &'a Path,
    staged: &'a Path,
    backup: &'a Path,
}

impl MacBundle {
    pub fn detect(launch: &Launch) -> Option<Arc<dyn Installer>> {
        let bundle = bundle_of(&launch.executable)?;
        let staging = writable_staging_beside(&bundle)?;
        Some(Arc::new(Self { bundle, staging }))
    }
}

impl Installer for MacBundle {
    fn staging_directory(&self) -> &Path {
        &self.staging
    }

    fn install(&self, download: &Path, version: &Version) -> Result<()> {
        let extracted = self.staging.join(EXTRACTED_DIRECTORY);
        extract_zip(download, &extracted)?;
        let staged = extracted.join(BUNDLE_NAME);
        validate_bundle(&staged, version)?;
        let swap = BundleSwap {
            installed: &self.bundle,
            staged: &staged,
            backup: &self.staging.join(PREVIOUS_BUNDLE),
        };
        swap_bundles(&swap, |from, to| fs::rename(from, to))
    }

    fn relaunch(&self) -> Relaunch {
        Relaunch::new(OPEN, [OsString::from(&self.bundle)])
    }
}

fn bundle_of(executable: &Path) -> Option<PathBuf> {
    let executables = executable.parent()?;
    let contents = executables.parent()?;
    let bundle = contents.parent()?;
    let is_inside_bundle = executables.ends_with(EXECUTABLES_DIRECTORY)
        && contents.ends_with(CONTENTS_DIRECTORY)
        && bundle.extension()? == BUNDLE_EXTENSION;
    is_inside_bundle.then(|| bundle.to_owned())
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let status = Command::new(DITTO)
        .args(["-x", "-k"])
        .arg(archive)
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(io_error("starting ditto"))?;
    match status.success() {
        true => Ok(()),
        false => Err(UpdateError::Extract(format!("ditto exited with {status}"))),
    }
}

fn validate_bundle(bundle: &Path, version: &Version) -> Result<()> {
    if !is_executable(&executable_in(bundle)) {
        return Err(UpdateError::InvalidBundle(
            "its executable is missing".to_owned(),
        ));
    }
    let plist = fs::read_to_string(bundle.join(CONTENTS_DIRECTORY).join(INFO_PLIST))
        .map_err(io_error("reading the Info.plist of the update"))?;
    let expected = version.to_string();
    match bundle_version(&plist) {
        Some(found) if found == expected => Ok(()),
        found => Err(UpdateError::InvalidBundle(format!(
            "it is version {} instead of {expected}",
            found.unwrap_or("unknown")
        ))),
    }
}

fn executable_in(bundle: &Path) -> PathBuf {
    bundle
        .join(CONTENTS_DIRECTORY)
        .join(EXECUTABLES_DIRECTORY)
        .join(EXECUTABLE_NAME)
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && metadata.permissions().mode() & EXECUTE_BITS != 0
    })
}

fn bundle_version(plist: &str) -> Option<&str> {
    let (_, after_key) = plist.split_once(VERSION_KEY)?;
    let (_, value) = after_key.split_once(VALUE_START)?;
    let (value, _) = value.split_once(VALUE_END)?;
    Some(value.trim())
}

fn swap_bundles(swap: &BundleSwap, rename: impl Fn(&Path, &Path) -> io::Result<()>) -> Result<()> {
    rename(swap.installed, swap.backup).map_err(io_error("moving the installed app aside"))?;
    let Err(replace_error) = rename(swap.staged, swap.installed) else {
        return Ok(());
    };
    match rename(swap.backup, swap.installed) {
        Ok(()) => Err(io_error("putting the new app in place")(replace_error)),
        Err(source) => Err(UpdateError::RollbackFailed {
            backup: swap.backup.to_owned(),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    const VERSION: Version = Version::new(2, 0, 0);

    fn write_bundle(bundle: &Path, version: &str) {
        let executable = executable_in(bundle);
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, version).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let plist = format!(
            "<plist><dict>\n<key>CFBundleName</key>\n<string>Cadence</string>\n\
             {VERSION_KEY}\n    <string>{version}</string>\n</dict></plist>\n"
        );
        fs::write(bundle.join(CONTENTS_DIRECTORY).join(INFO_PLIST), plist).unwrap();
    }

    fn installed_version(bundle: &Path) -> String {
        fs::read_to_string(executable_in(bundle)).unwrap()
    }

    struct Bundles {
        _directory: TempDir,
        installed: PathBuf,
        staged: PathBuf,
        backup: PathBuf,
    }

    impl Bundles {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let installed = directory.path().join("Cadence.app");
            let staged = directory.path().join("staged.app");
            let backup = directory.path().join("backup.app");
            write_bundle(&installed, "1.0.0");
            write_bundle(&staged, "2.0.0");
            Self {
                _directory: directory,
                installed,
                staged,
                backup,
            }
        }

        fn swap(&self) -> BundleSwap<'_> {
            BundleSwap {
                installed: &self.installed,
                staged: &self.staged,
                backup: &self.backup,
            }
        }
    }

    fn real_rename(from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    #[test]
    fn given_a_staged_bundle_when_swapping_then_it_replaces_the_installed_one() {
        let bundles = Bundles::new();

        swap_bundles(&bundles.swap(), real_rename).unwrap();

        assert_eq!(installed_version(&bundles.installed), "2.0.0");
        assert_eq!(installed_version(&bundles.backup), "1.0.0");
        assert!(!bundles.staged.exists());
    }

    #[test]
    fn given_the_new_bundle_cannot_be_moved_when_swapping_then_the_old_one_is_put_back() {
        let bundles = Bundles::new();
        fs::remove_dir_all(&bundles.staged).unwrap();

        let error = swap_bundles(&bundles.swap(), real_rename).unwrap_err();

        assert!(matches!(error, UpdateError::Io { .. }), "{error}");
        assert_eq!(installed_version(&bundles.installed), "1.0.0");
        assert!(!bundles.backup.exists());
    }

    #[test]
    fn given_the_installed_bundle_cannot_be_moved_when_swapping_then_nothing_changes() {
        let bundles = Bundles::new();
        let missing_parent = bundles.backup.join("missing").join("backup.app");

        let swap = BundleSwap {
            backup: &missing_parent,
            ..bundles.swap()
        };
        let error = swap_bundles(&swap, real_rename).unwrap_err();

        assert!(matches!(error, UpdateError::Io { .. }), "{error}");
        assert_eq!(installed_version(&bundles.installed), "1.0.0");
        assert_eq!(installed_version(&bundles.staged), "2.0.0");
    }

    #[test]
    fn given_the_rollback_also_fails_when_swapping_then_the_backup_location_is_reported() {
        let bundles = Bundles::new();
        let refuse_to_restore = |from: &Path, to: &Path| match to == bundles.installed {
            true => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            false => fs::rename(from, to),
        };

        let error = swap_bundles(&bundles.swap(), refuse_to_restore).unwrap_err();

        let UpdateError::RollbackFailed { backup, .. } = error else {
            panic!("expected a failed rollback, got {error}");
        };
        assert_eq!(backup, bundles.backup);
        assert_eq!(installed_version(&bundles.backup), "1.0.0");
    }

    #[test]
    fn given_a_bundle_of_the_release_when_validating_then_it_is_accepted() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("Cadence.app");
        write_bundle(&bundle, "2.0.0");

        validate_bundle(&bundle, &VERSION).unwrap();
    }

    #[test]
    fn given_another_version_when_validating_then_it_is_refused_and_named() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("Cadence.app");
        write_bundle(&bundle, "1.9.0");

        let error = validate_bundle(&bundle, &VERSION).unwrap_err();

        assert!(matches!(error, UpdateError::InvalidBundle(_)), "{error}");
        assert!(error.to_string().contains("1.9.0"), "{error}");
    }

    #[test]
    fn given_a_missing_executable_when_validating_then_it_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("Cadence.app");
        write_bundle(&bundle, "2.0.0");
        fs::remove_file(executable_in(&bundle)).unwrap();

        let error = validate_bundle(&bundle, &VERSION).unwrap_err();

        assert!(matches!(error, UpdateError::InvalidBundle(_)), "{error}");
    }

    #[test]
    fn given_an_executable_without_the_execute_bit_when_validating_then_it_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("Cadence.app");
        write_bundle(&bundle, "2.0.0");
        let plain_file = fs::Permissions::from_mode(0o644);
        fs::set_permissions(executable_in(&bundle), plain_file).unwrap();

        let error = validate_bundle(&bundle, &VERSION).unwrap_err();

        assert!(matches!(error, UpdateError::InvalidBundle(_)), "{error}");
    }

    #[test]
    fn given_a_plist_without_a_version_when_validating_then_it_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let bundle = directory.path().join("Cadence.app");
        write_bundle(&bundle, "2.0.0");
        fs::write(bundle.join(CONTENTS_DIRECTORY).join(INFO_PLIST), "<plist/>").unwrap();

        let error = validate_bundle(&bundle, &VERSION).unwrap_err();

        assert!(error.to_string().contains("unknown"), "{error}");
    }

    #[test]
    fn given_an_executable_inside_a_bundle_when_resolving_then_the_bundle_is_found() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/Cadence.app/Contents/MacOS/cadence"
            )),
            Some(PathBuf::from("/Applications/Cadence.app"))
        );
    }

    #[test]
    fn given_a_renamed_bundle_when_resolving_then_its_own_name_is_kept() {
        assert_eq!(
            bundle_of(Path::new("/Users/me/Apps/Music.app/Contents/MacOS/cadence")),
            Some(PathBuf::from("/Users/me/Apps/Music.app"))
        );
    }

    #[test]
    fn given_a_development_build_when_resolving_then_there_is_no_bundle() {
        assert_eq!(bundle_of(Path::new("/src/rust/target/debug/cadence")), None);
        assert_eq!(bundle_of(Path::new("/usr/bin/cadence")), None);
        assert_eq!(bundle_of(Path::new("/cadence")), None);
    }

    #[test]
    fn given_a_directory_that_only_looks_like_a_bundle_when_resolving_then_it_is_refused() {
        assert_eq!(
            bundle_of(Path::new("/opt/Cadence/Contents/MacOS/cadence")),
            None
        );
    }
}
