use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use cadence_updater::{
    Installer, Launch, Offer, Platform, Relaunch, ReleaseFeed, STAGING_DIRECTORY, UpdateError,
    Updater, installer_for,
};
use tempfile::TempDir;
use wiremock::{MockServer, ResponseTemplate};

use crate::support::{CHECKSUMS, INSTALLED, checksums_of, mount_asset, mount_latest, release_json};

const WINDOWS: Platform = Platform::Windows("x86_64");
const ZIP_NAME: &str = "Cadence-2.0.0-windows-x86_64.zip";
const LATEST_PATH: &str = "/releases/latest";
const EXECUTABLE: &str = "cadence.exe";
const LIBRARY: &str = "libmpv-2.dll";

struct Installed {
    directory: TempDir,
    executable: PathBuf,
}

impl Installed {
    fn named(executable_name: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("Cadence");
        fs::create_dir(&folder).unwrap();
        let executable = folder.join(executable_name);
        fs::write(&executable, "cadence 1.0.0").unwrap();
        fs::write(folder.join(LIBRARY), "libmpv 1.0.0").unwrap();
        Self {
            directory,
            executable,
        }
    }

    fn new() -> Self {
        Self::named(EXECUTABLE)
    }

    fn folder(&self) -> &Path {
        self.executable.parent().unwrap()
    }

    fn launch(&self) -> Launch {
        Launch {
            executable: self.executable.clone(),
            ..Launch::default()
        }
    }

    fn installer(&self) -> Arc<dyn Installer> {
        installer_for(&self.launch()).unwrap()
    }

    fn staging(&self) -> PathBuf {
        self.folder().join(STAGING_DIRECTORY)
    }

    fn versions(&self) -> (String, String) {
        (
            fs::read_to_string(&self.executable).unwrap(),
            fs::read_to_string(self.folder().join(LIBRARY)).unwrap(),
        )
    }

    fn zip_of(&self, files: &[&str]) -> Vec<u8> {
        let source = self.directory.path().join("source");
        fs::create_dir(&source).unwrap();
        for name in files {
            fs::write(source.join(name), format!("{name} 2.0.0")).unwrap();
        }
        let archive = self.directory.path().join("archive.zip");
        let status = Command::new("tar.exe")
            .args(["-a", "-c", "-f"])
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .args(files)
            .status()
            .unwrap();
        assert!(status.success(), "tar exited with {status}");
        let bytes = fs::read(&archive).unwrap();
        fs::remove_dir_all(source).unwrap();
        fs::remove_file(archive).unwrap();
        bytes
    }
}

fn updater_for(server: &MockServer, installer: Option<Arc<dyn Installer>>) -> Updater {
    let feed = ReleaseFeed::new(
        reqwest::Client::new(),
        format!("{}{LATEST_PATH}", server.uri()),
    );
    Updater::new(feed, WINDOWS, INSTALLED, installer)
}

async fn publish(server: &MockServer, zip: &[u8]) -> Offer {
    let checksums = checksums_of(&[(ZIP_NAME, zip)]);
    let body = release_json(
        server,
        "v2.0.0",
        &[(ZIP_NAME, zip.len()), (CHECKSUMS, checksums.len())],
    );
    mount_latest(server, ResponseTemplate::new(200).set_body_json(body)).await;
    mount_asset(server, ZIP_NAME, zip).await;
    mount_asset(server, CHECKSUMS, checksums.as_bytes()).await;
    updater_for(server, None).check().await.unwrap().unwrap()
}

async fn update(
    server: &MockServer,
    installed: &Installed,
    zip: &[u8],
) -> Result<Relaunch, UpdateError> {
    let offer = publish(server, zip).await;
    updater_for(server, Some(installed.installer()))
        .install(&offer, |_progress| {})
        .await
}

#[test]
fn given_a_portable_folder_when_detecting_then_it_can_be_updated_from_inside_it() {
    let installed = Installed::new();

    let installer = installed.installer();

    assert_eq!(installer.staging_directory(), installed.staging());
    assert_eq!(
        installer.relaunch(),
        Relaunch::new(&installed.executable, [])
    );
}

#[test]
fn given_an_executable_without_the_player_library_beside_it_when_detecting_then_it_falls_back_to_the_release_page()
 {
    let installed = Installed::new();
    fs::remove_file(installed.folder().join(LIBRARY)).unwrap();

    assert!(installer_for(&installed.launch()).is_none());
}

#[tokio::test]
async fn given_a_published_release_when_updating_then_the_executable_and_the_player_library_are_replaced()
 {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = installed.zip_of(&[EXECUTABLE, LIBRARY]);

    let relaunch = update(&server, &installed, &zip).await.unwrap();

    assert_eq!(
        installed.versions(),
        (
            "cadence.exe 2.0.0".to_owned(),
            "libmpv-2.dll 2.0.0".to_owned()
        )
    );
    assert_eq!(relaunch, Relaunch::new(&installed.executable, []));
    assert!(!installed.staging().exists());
}

#[tokio::test]
async fn given_a_renamed_executable_when_updating_then_the_new_one_keeps_its_name() {
    let server = MockServer::start().await;
    let installed = Installed::named("Music.exe");
    let zip = installed.zip_of(&[EXECUTABLE, LIBRARY]);

    update(&server, &installed, &zip).await.unwrap();

    assert_eq!(installed.versions().0, "cadence.exe 2.0.0");
    assert!(!installed.folder().join(EXECUTABLE).exists());
}

#[tokio::test]
async fn given_a_zip_without_the_player_library_when_updating_then_the_installed_folder_is_untouched()
 {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = installed.zip_of(&[EXECUTABLE]);

    let error = update(&server, &installed, &zip).await.unwrap_err();

    assert!(matches!(error, UpdateError::InvalidBundle(_)), "{error}");
    assert_eq!(
        installed.versions(),
        ("cadence 1.0.0".to_owned(), "libmpv 1.0.0".to_owned())
    );
    assert!(!installed.staging().exists());
}
