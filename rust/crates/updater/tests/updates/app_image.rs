use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use cadence_updater::{Launch, Relaunch, STAGING_DIRECTORY, Version, installer_for};
use tempfile::TempDir;
use wiremock::MockServer;

use crate::support::{PAYLOAD, ReadOnlyDirectory, published_release, updater};

const OLD_IMAGE: &[u8] = b"the bytes of the running version";
const NEW_VERSION: Version = Version::new(2, 0, 0);

struct Mounted {
    directory: TempDir,
    image: PathBuf,
}

impl Mounted {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let applications = directory.path().join("Applications");
        fs::create_dir(&applications).unwrap();
        let image = applications.join("Cadence.AppImage");
        fs::write(&image, OLD_IMAGE).unwrap();
        fs::set_permissions(&image, fs::Permissions::from_mode(0o755)).unwrap();
        Self { directory, image }
    }

    fn app_dir(&self) -> PathBuf {
        self.directory.path().join(".mount_Cadence")
    }

    fn launch(&self) -> Launch {
        Launch {
            executable: self.app_dir().join("usr/bin/cadence"),
            app_image: Some(self.image.clone()),
            app_dir: Some(self.app_dir()),
        }
    }

    fn staging(&self) -> PathBuf {
        self.image.parent().unwrap().join(STAGING_DIRECTORY)
    }

    fn write_download(&self) -> PathBuf {
        fs::create_dir_all(self.staging()).unwrap();
        let download = self.staging().join("download");
        fs::write(&download, PAYLOAD).unwrap();
        download
    }
}

#[test]
fn given_a_running_appimage_when_detecting_then_it_can_be_replaced() {
    let mounted = Mounted::new();

    let installer = installer_for(&mounted.launch()).unwrap();

    assert_eq!(installer.staging_directory(), mounted.staging());
    assert_eq!(installer.relaunch(), Relaunch::new(&mounted.image, []));
}

#[test]
fn given_an_appimage_variable_inherited_by_a_source_build_when_detecting_then_it_is_ignored() {
    let mounted = Mounted::new();
    let launch = Launch {
        executable: PathBuf::from("/usr/bin/cadence"),
        ..mounted.launch()
    };

    assert!(installer_for(&launch).is_none());
    assert_eq!(fs::read(&mounted.image).unwrap(), OLD_IMAGE);
}

#[test]
fn given_an_appimage_variable_without_a_mount_when_detecting_then_it_is_ignored() {
    let mounted = Mounted::new();
    let launch = Launch {
        app_dir: None,
        ..mounted.launch()
    };

    assert!(installer_for(&launch).is_none());
}

#[test]
fn given_no_appimage_when_detecting_then_there_is_nothing_to_replace() {
    let launch = Launch {
        executable: PathBuf::from("/usr/bin/cadence"),
        ..Launch::default()
    };

    assert!(installer_for(&launch).is_none());
}

#[test]
fn given_a_read_only_directory_when_detecting_then_it_falls_back_to_the_release_page() {
    let mounted = Mounted::new();
    let applications = mounted.image.parent().unwrap();
    let read_only = ReadOnlyDirectory::new(applications);
    if !read_only.is_enforced() {
        eprintln!("skipped: this user ignores directory permissions");
        return;
    }

    assert!(installer_for(&mounted.launch()).is_none());
}

#[test]
fn given_a_parent_that_is_not_a_directory_when_detecting_then_it_cannot_be_replaced() {
    let mounted = Mounted::new();
    let launch = Launch {
        app_image: Some(Path::new(&mounted.image).join("nested.AppImage")),
        ..mounted.launch()
    };

    assert!(installer_for(&launch).is_none());
}

#[test]
fn given_a_leftover_staging_directory_when_detecting_then_it_is_removed() {
    let mounted = Mounted::new();
    mounted.write_download();

    assert!(installer_for(&mounted.launch()).is_some());

    assert!(!mounted.staging().exists());
}

#[test]
fn given_a_verified_download_when_installing_then_the_file_is_replaced_and_stays_executable() {
    let mounted = Mounted::new();
    let installer = installer_for(&mounted.launch()).unwrap();
    let download = mounted.write_download();

    installer.install(&download, &NEW_VERSION).unwrap();

    assert_eq!(fs::read(&mounted.image).unwrap(), PAYLOAD);
    let mode = fs::metadata(&mounted.image).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert!(!download.exists());
}

#[test]
fn given_the_replacement_fails_when_installing_then_the_old_file_stays_in_place() {
    let mounted = Mounted::new();
    let installer = installer_for(&mounted.launch()).unwrap();
    let download = mounted.write_download();
    let applications = mounted.image.parent().unwrap();
    let read_only = ReadOnlyDirectory::new(applications);
    if !read_only.is_enforced() {
        eprintln!("skipped: this user ignores directory permissions");
        return;
    }

    let error = installer.install(&download, &NEW_VERSION);

    assert!(error.is_err());
    assert_eq!(fs::read(&mounted.image).unwrap(), OLD_IMAGE);
}

#[test]
fn given_the_download_vanished_when_installing_then_the_old_file_stays_in_place() {
    let mounted = Mounted::new();
    let installer = installer_for(&mounted.launch()).unwrap();

    let error = installer.install(&mounted.staging().join("missing"), &NEW_VERSION);

    assert!(error.is_err());
    assert_eq!(fs::read(&mounted.image).unwrap(), OLD_IMAGE);
}

#[tokio::test]
async fn given_a_published_release_when_updating_then_the_appimage_is_replaced_end_to_end() {
    let server = MockServer::start().await;
    let mounted = Mounted::new();
    let offer = published_release(&server, PAYLOAD).await;
    let installer = installer_for(&mounted.launch());

    let relaunch = updater(&server, installer)
        .install(&offer, |_progress| {})
        .await
        .unwrap();

    assert_eq!(fs::read(&mounted.image).unwrap(), PAYLOAD);
    assert_eq!(relaunch, Relaunch::new(&mounted.image, []));
    assert!(!mounted.staging().exists());
}

#[tokio::test]
async fn given_a_corrupted_release_when_updating_then_the_appimage_is_left_untouched() {
    let server = MockServer::start().await;
    let mounted = Mounted::new();
    let offer = published_release(&server, b"tampered bytes").await;
    let installer = installer_for(&mounted.launch());

    let outcome = updater(&server, installer)
        .install(&offer, |_progress| {})
        .await;

    assert!(outcome.is_err());
    assert_eq!(fs::read(&mounted.image).unwrap(), OLD_IMAGE);
    assert!(!mounted.staging().exists());
}
