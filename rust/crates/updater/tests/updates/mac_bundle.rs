use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use cadence_updater::{
    Installer, Launch, Offer, Platform, Relaunch, ReleaseFeed, STAGING_DIRECTORY, UpdateError,
    Updater, Version, installer_for,
};
use tempfile::TempDir;
use wiremock::{MockServer, ResponseTemplate};

use crate::support::{
    CHECKSUMS, INSTALLED, ReadOnlyDirectory, checksums_of, mount_asset, mount_latest, release_json,
    sha256_hex,
};

const MAC: Platform = Platform::MacOs("aarch64");
const ZIP_NAME: &str = "Cadence-2.0.0-macos-arm64.zip";
const REAL_EXECUTABLE: &str = "/usr/bin/true";
const LATEST_PATH: &str = "/releases/latest";

fn write_info_plist(contents: &Path, version: &str) {
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>cadence</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.douglasmarq.cadence.test</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
</dict>
</plist>
"#
    );
    fs::write(contents.join("Info.plist"), plist).unwrap();
}

fn run(program: &str, arguments: &[&std::ffi::OsStr]) {
    let output = Command::new(program).args(arguments).output().unwrap();
    assert!(output.status.success(), "{program} failed: {output:?}");
}

fn write_signed_bundle(bundle: &Path, version: &str) {
    let contents = bundle.join("Contents");
    let executables = contents.join("MacOS");
    let frameworks = contents.join("Frameworks");
    let resources = contents.join("Resources");
    fs::create_dir_all(&executables).unwrap();
    fs::create_dir_all(&frameworks).unwrap();
    fs::create_dir_all(&resources).unwrap();
    fs::copy(REAL_EXECUTABLE, executables.join("cadence")).unwrap();
    fs::set_permissions(
        executables.join("cadence"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::copy(REAL_EXECUTABLE, frameworks.join("libmpv.2.dylib")).unwrap();
    symlink("libmpv.2.dylib", frameworks.join("libmpv.dylib")).unwrap();
    fs::write(resources.join("version.txt"), version).unwrap();
    write_info_plist(&contents, version);
    run(
        "codesign",
        &[
            "--force".as_ref(),
            "--deep".as_ref(),
            "--sign".as_ref(),
            "-".as_ref(),
            bundle.as_os_str(),
        ],
    );
}

fn zip_of_bundle(directory: &Path, version: &str) -> Vec<u8> {
    let source = directory.join("source");
    let bundle = source.join("Cadence.app");
    write_signed_bundle(&bundle, version);
    let archive = directory.join("archive.zip");
    run(
        "ditto",
        &[
            "-c".as_ref(),
            "-k".as_ref(),
            "--keepParent".as_ref(),
            bundle.as_os_str(),
            archive.as_os_str(),
        ],
    );
    let bytes = fs::read(&archive).unwrap();
    fs::remove_dir_all(source).unwrap();
    fs::remove_file(archive).unwrap();
    bytes
}

struct Installed {
    directory: TempDir,
    bundle: PathBuf,
}

impl Installed {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let applications = directory.path().join("Applications");
        fs::create_dir(&applications).unwrap();
        let bundle = applications.join("Cadence.app");
        write_signed_bundle(&bundle, "1.0.0");
        Self { directory, bundle }
    }

    fn launch(&self) -> Launch {
        Launch {
            executable: self.bundle.join("Contents/MacOS/cadence"),
            ..Launch::default()
        }
    }

    fn installer(&self) -> Arc<dyn Installer> {
        installer_for(&self.launch()).unwrap()
    }

    fn staging(&self) -> PathBuf {
        self.bundle.parent().unwrap().join(STAGING_DIRECTORY)
    }

    fn version(&self) -> String {
        let plist = fs::read_to_string(self.bundle.join("Contents/Info.plist")).unwrap();
        let (_, value) = plist
            .split_once("<key>CFBundleShortVersionString</key>")
            .unwrap();
        let (_, value) = value.split_once("<string>").unwrap();
        value.split_once("</string>").unwrap().0.to_owned()
    }

    fn signature_is_valid(&self) -> bool {
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&self.bundle)
            .status()
            .unwrap()
            .success()
    }
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

fn updater_for(server: &MockServer, installer: Option<Arc<dyn Installer>>) -> Updater {
    let feed = ReleaseFeed::new(
        reqwest::Client::new(),
        format!("{}{LATEST_PATH}", server.uri()),
    );
    Updater::new(feed, MAC, INSTALLED, installer)
}

async fn update(
    server: &MockServer,
    installed: &Installed,
    offer: &Offer,
) -> Result<Relaunch, UpdateError> {
    updater_for(server, Some(installed.installer()))
        .install(offer, |_progress| {})
        .await
}

#[test]
fn given_a_bundle_in_a_writable_directory_when_detecting_then_it_can_be_replaced() {
    let installed = Installed::new();

    let installer = installed.installer();

    assert_eq!(installer.staging_directory(), installed.staging());
    let bundle = OsString::from(&installed.bundle);
    assert_eq!(
        installer.relaunch(),
        Relaunch::new("/usr/bin/open", [bundle])
    );
}

#[test]
fn given_a_development_build_when_detecting_then_it_falls_back_to_the_release_page() {
    let launch = Launch {
        executable: PathBuf::from("/src/rust/target/debug/cadence"),
        ..Launch::default()
    };

    assert!(installer_for(&launch).is_none());
}

#[test]
fn given_a_read_only_directory_when_detecting_then_it_falls_back_to_the_release_page() {
    let installed = Installed::new();
    let read_only = ReadOnlyDirectory::new(installed.bundle.parent().unwrap());
    if !read_only.is_enforced() {
        eprintln!("skipped: this user ignores directory permissions");
        return;
    }

    assert!(installer_for(&installed.launch()).is_none());
}

#[tokio::test]
async fn given_a_published_release_when_updating_then_the_whole_bundle_is_replaced() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = zip_of_bundle(installed.directory.path(), "2.0.0");
    let offer = publish(&server, &zip).await;

    let relaunch = update(&server, &installed, &offer).await.unwrap();

    assert_eq!(installed.version(), "2.0.0");
    let resource = installed.bundle.join("Contents/Resources/version.txt");
    assert_eq!(fs::read_to_string(resource).unwrap(), "2.0.0");
    let bundle = OsString::from(&installed.bundle);
    assert_eq!(relaunch, Relaunch::new("/usr/bin/open", [bundle]));
    assert!(!installed.staging().exists());
}

#[tokio::test]
async fn given_a_published_release_when_updating_then_symlinks_and_signature_survive() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = zip_of_bundle(installed.directory.path(), "2.0.0");
    let offer = publish(&server, &zip).await;

    update(&server, &installed, &offer).await.unwrap();

    let link = installed.bundle.join("Contents/Frameworks/libmpv.dylib");
    assert_eq!(fs::read_link(link).unwrap(), Path::new("libmpv.2.dylib"));
    assert!(installed.signature_is_valid());
}

#[tokio::test]
async fn given_a_zip_of_another_version_when_updating_then_the_installed_bundle_is_untouched() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = zip_of_bundle(installed.directory.path(), "1.5.0");
    let offer = publish(&server, &zip).await;

    let error = update(&server, &installed, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::InvalidBundle(_)), "{error}");
    assert_eq!(installed.version(), "1.0.0");
    assert!(installed.signature_is_valid());
    assert!(!installed.staging().exists());
}

#[tokio::test]
async fn given_bytes_that_are_no_zip_when_updating_then_the_installed_bundle_is_untouched() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let offer = publish(&server, b"this is not a zip archive").await;

    let error = update(&server, &installed, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::Extract(_)), "{error}");
    assert_eq!(installed.version(), "1.0.0");
    assert!(!installed.staging().exists());
}

#[tokio::test]
async fn given_a_zip_that_fails_its_checksum_when_updating_then_it_is_never_extracted() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = zip_of_bundle(installed.directory.path(), "2.0.0");
    let mut offer = publish(&server, &zip).await;
    let tampered = format!("{}  {ZIP_NAME}\n", sha256_hex(b"something else"));
    mount_asset(&server, "tampered-listing", tampered.as_bytes()).await;
    offer.checksums.browser_download_url = format!("{}/download/tampered-listing", server.uri());

    let error = update(&server, &installed, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::ChecksumMismatch(_)), "{error}");
    assert_eq!(installed.version(), "1.0.0");
    assert!(!installed.staging().exists());
}

#[tokio::test]
async fn given_a_renamed_bundle_when_updating_then_it_keeps_its_name() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let renamed = installed.bundle.with_file_name("Music.app");
    fs::rename(&installed.bundle, &renamed).unwrap();
    let renamed = Installed {
        directory: installed.directory,
        bundle: renamed,
    };
    let zip = zip_of_bundle(renamed.directory.path(), "2.0.0");
    let offer = publish(&server, &zip).await;

    update(&server, &renamed, &offer).await.unwrap();

    assert_eq!(renamed.version(), "2.0.0");
    assert!(!renamed.bundle.with_file_name("Cadence.app").exists());
}

#[tokio::test]
async fn given_the_new_version_when_checking_again_then_nothing_is_offered() {
    let server = MockServer::start().await;
    let installed = Installed::new();
    let zip = zip_of_bundle(installed.directory.path(), "2.0.0");
    publish(&server, &zip).await;
    let feed = ReleaseFeed::new(
        reqwest::Client::new(),
        format!("{}{LATEST_PATH}", server.uri()),
    );

    let updater = Updater::new(feed, MAC, Version::new(2, 0, 0), None);

    assert_eq!(updater.check().await.unwrap(), None);
}
