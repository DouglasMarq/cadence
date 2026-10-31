use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cadence_updater::{
    Installer, Offer, Platform, Relaunch, ReleaseFeed, Result, UpdateError, Updater, Version,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const INSTALLED: Version = Version::new(1, 0, 0);
pub const LINUX: Platform = Platform::Linux("x86_64");
pub const NEW_TAG: &str = "v2.0.0";
pub const NEW_ASSET: &str = "Cadence-2.0.0-x86_64.AppImage";
pub const CHECKSUMS: &str = "SHA256SUMS";
pub const LATEST_PATH: &str = "/releases/latest";
pub const PAYLOAD: &[u8] = b"the bytes of the new version of the app";

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn checksums_of(entries: &[(&str, &[u8])]) -> String {
    entries
        .iter()
        .map(|(name, bytes)| format!("{}  {name}\n", sha256_hex(bytes)))
        .collect()
}

pub fn asset_url(server: &MockServer, name: &str) -> String {
    format!("{}/download/{name}", server.uri())
}

pub fn asset_json(url: &str, name: &str, size: usize) -> Value {
    json!({ "name": name, "browser_download_url": url, "size": size, "state": "uploaded" })
}

pub fn release_json(server: &MockServer, tag: &str, assets: &[(&str, usize)]) -> Value {
    let assets: Vec<Value> = assets
        .iter()
        .map(|(name, size)| asset_json(&asset_url(server, name), name, *size))
        .collect();
    json!({
        "tag_name": tag,
        "html_url": format!("https://releases.example/{tag}"),
        "draft": false,
        "prerelease": false,
        "assets": assets,
    })
}

pub async fn mount_latest(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(LATEST_PATH))
        .respond_with(response)
        .mount(server)
        .await;
}

pub async fn mount_asset(server: &MockServer, name: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(format!("/download/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

pub fn feed_of(server: &MockServer) -> ReleaseFeed {
    let latest_url = format!("{}{LATEST_PATH}", server.uri());
    ReleaseFeed::new(reqwest::Client::new(), latest_url)
}

pub fn updater(server: &MockServer, installer: Option<Arc<dyn Installer>>) -> Updater {
    Updater::new(feed_of(server), LINUX, INSTALLED, installer)
}

pub async fn published_release(server: &MockServer, asset_body: &[u8]) -> Offer {
    let checksums = checksums_of(&[(NEW_ASSET, PAYLOAD)]);
    let body = release_json(
        server,
        NEW_TAG,
        &[(NEW_ASSET, PAYLOAD.len()), (CHECKSUMS, checksums.len())],
    );
    mount_latest(server, ResponseTemplate::new(200).set_body_json(body)).await;
    mount_asset(server, NEW_ASSET, asset_body).await;
    mount_asset(server, CHECKSUMS, checksums.as_bytes()).await;
    updater(server, None).check().await.unwrap().unwrap()
}

pub enum InstallResult {
    Succeeds,
    Fails,
    FailsToRollBack,
}

pub struct RecordingInstaller {
    staging: PathBuf,
    result: InstallResult,
    installed: Mutex<Option<Vec<u8>>>,
}

impl RecordingInstaller {
    pub fn new(staging: &Path, result: InstallResult) -> Arc<Self> {
        Arc::new(Self {
            staging: staging.to_owned(),
            result,
            installed: Mutex::new(None),
        })
    }

    pub fn installed(&self) -> Option<Vec<u8>> {
        self.installed.lock().unwrap().clone()
    }
}

impl Installer for RecordingInstaller {
    fn staging_directory(&self) -> &Path {
        &self.staging
    }

    fn install(&self, download: &Path, _version: &Version) -> Result<()> {
        *self.installed.lock().unwrap() = Some(fs::read(download).unwrap());
        match self.result {
            InstallResult::Succeeds => Ok(()),
            InstallResult::Fails => Err(UpdateError::Extract("ditto exited with 1".to_owned())),
            InstallResult::FailsToRollBack => Err(UpdateError::RollbackFailed {
                backup: self.staging.join("previous.app"),
                source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            }),
        }
    }

    fn relaunch(&self) -> Relaunch {
        Relaunch::new("cadence-relaunched", [])
    }
}

pub struct ReadOnlyDirectory<'a>(&'a Path);

impl<'a> ReadOnlyDirectory<'a> {
    pub fn new(directory: &'a Path) -> Self {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
        Self(directory)
    }

    pub fn is_enforced(&self) -> bool {
        let probe = self.0.join("write-probe");
        let is_refused = fs::write(&probe, "").is_err();
        drop(fs::remove_file(&probe));
        is_refused
    }
}

impl Drop for ReadOnlyDirectory<'_> {
    fn drop(&mut self) {
        fs::set_permissions(self.0, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
