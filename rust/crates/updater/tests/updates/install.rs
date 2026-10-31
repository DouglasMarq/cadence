use std::path::Path;
use std::sync::{Arc, Mutex};

use cadence_updater::{Offer, Progress, Relaunch, ReleaseAsset, UpdateError};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use wiremock::MockServer;

use crate::support::{
    CHECKSUMS, InstallResult, NEW_ASSET, PAYLOAD, RecordingInstaller, checksums_of, mount_asset,
    published_release, updater,
};

struct Staged {
    directory: TempDir,
    installer: Arc<RecordingInstaller>,
}

impl Staged {
    fn new(result: InstallResult) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let installer = RecordingInstaller::new(&directory.path().join("staging"), result);
        Self {
            directory,
            installer,
        }
    }

    fn staging(&self) -> std::path::PathBuf {
        self.directory.path().join("staging")
    }
}

async fn install(
    server: &MockServer,
    staged: &Staged,
    offer: &Offer,
) -> Result<Relaunch, UpdateError> {
    let installer = Arc::clone(&staged.installer);
    updater(server, Some(installer))
        .install(offer, |_progress| {})
        .await
}

fn assert_untouched_and_clean(staged: &Staged) {
    assert_eq!(staged.installer.installed(), None);
    assert!(!staged.staging().exists());
}

#[tokio::test]
async fn given_a_verified_download_when_installing_then_the_installer_gets_its_bytes() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let offer = published_release(&server, PAYLOAD).await;

    let relaunch = install(&server, &staged, &offer).await.unwrap();

    assert_eq!(staged.installer.installed().as_deref(), Some(PAYLOAD));
    assert_eq!(relaunch, Relaunch::new("cadence-relaunched", []));
    assert!(!staged.staging().exists());
}

#[tokio::test]
async fn given_a_download_when_installing_then_progress_rises_and_ends_with_installing() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let offer = published_release(&server, PAYLOAD).await;
    let reported = Mutex::new(Vec::new());

    updater(&server, Some(Arc::clone(&staged.installer) as _))
        .install(&offer, |progress| reported.lock().unwrap().push(progress))
        .await
        .unwrap();

    let reported = reported.into_inner().unwrap();
    assert_eq!(reported.last(), Some(&Progress::Installing));
    assert_eq!(
        reported.iter().rev().nth(1),
        Some(&Progress::Downloading(100))
    );
}

#[tokio::test]
async fn given_a_corrupted_download_when_installing_then_nothing_is_installed() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let corrupted = PAYLOAD.iter().map(|byte| byte ^ 1).collect::<Vec<u8>>();
    let offer = published_release(&server, &corrupted).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::ChecksumMismatch(ref name) if name == NEW_ASSET));
    assert_untouched_and_clean(&staged);
}

#[tokio::test]
async fn given_a_download_shorter_than_published_when_installing_then_nothing_is_installed() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let truncated = PAYLOAD.get(..PAYLOAD.len() / 2).unwrap();
    let offer = published_release(&server, truncated).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::ChecksumMismatch(_)), "{error}");
    assert_untouched_and_clean(&staged);
}

async fn serve_connection_that_drops_midway(declared_length: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _peer) = listener.accept().await.unwrap();
        let mut request = [0; 1024];
        let _read = socket.read(&mut request).await.unwrap();
        let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {declared_length}\r\n\r\n");
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(b"only the first bytes").await.unwrap();
    });
    format!("http://{address}/{NEW_ASSET}")
}

#[tokio::test]
async fn given_the_connection_drops_during_the_download_when_installing_then_nothing_is_installed()
{
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let mut offer = published_release(&server, PAYLOAD).await;
    offer.asset.browser_download_url = serve_connection_that_drops_midway(PAYLOAD.len()).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::Network(_)), "{error}");
    assert_untouched_and_clean(&staged);
}

#[tokio::test]
async fn given_more_bytes_than_published_when_installing_then_the_download_is_abandoned() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let oversized = PAYLOAD.repeat(4);
    let mut offer = published_release(&server, PAYLOAD).await;
    offer.asset = ReleaseAsset {
        browser_download_url: format!("{}/download/oversized", server.uri()),
        ..offer.asset
    };
    mount_asset(&server, "oversized", &oversized).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::TooLarge(_)), "{error}");
    assert_untouched_and_clean(&staged);
}

#[tokio::test]
async fn given_checksums_without_the_asset_when_installing_then_nothing_is_downloaded() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let mut offer = published_release(&server, PAYLOAD).await;
    let other_listing = checksums_of(&[("another-asset", PAYLOAD)]);
    mount_asset(&server, "other-listing", other_listing.as_bytes()).await;
    offer.checksums = ReleaseAsset {
        name: CHECKSUMS.to_owned(),
        browser_download_url: format!("{}/download/other-listing", server.uri()),
        size: 1,
    };

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::MissingChecksum(_)), "{error}");
    assert_untouched_and_clean(&staged);
}

#[tokio::test]
async fn given_a_missing_asset_when_installing_then_the_status_is_reported() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Succeeds);
    let mut offer = published_release(&server, PAYLOAD).await;
    offer.asset.browser_download_url = format!("{}/download/removed", server.uri());

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(
        matches!(error, UpdateError::Status(status) if status == 404),
        "{error}"
    );
    assert_untouched_and_clean(&staged);
}

#[tokio::test]
async fn given_a_failing_installer_when_installing_then_the_staging_directory_is_removed() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::Fails);
    let offer = published_release(&server, PAYLOAD).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(matches!(error, UpdateError::Extract(_)), "{error}");
    assert!(!staged.staging().exists());
}

#[tokio::test]
async fn given_the_rollback_failed_when_installing_then_the_old_app_in_staging_is_kept() {
    let server = MockServer::start().await;
    let staged = Staged::new(InstallResult::FailsToRollBack);
    let offer = published_release(&server, PAYLOAD).await;

    let error = install(&server, &staged, &offer).await.unwrap_err();

    assert!(
        matches!(error, UpdateError::RollbackFailed { .. }),
        "{error}"
    );
    assert!(staged.staging().exists());
}

#[tokio::test]
async fn given_an_install_that_cannot_be_replaced_when_installing_then_nothing_is_requested() {
    let server = MockServer::start().await;
    let offer = published_release(&server, PAYLOAD).await;
    let requests_before = server.received_requests().await.unwrap().len();

    let error = updater(&server, None)
        .install(&offer, |_progress| {})
        .await
        .unwrap_err();

    assert!(matches!(error, UpdateError::NotInstallable), "{error}");
    let requests_after = server.received_requests().await.unwrap().len();
    assert_eq!(requests_before, requests_after);
}

#[tokio::test]
async fn given_a_staging_path_that_cannot_be_created_when_installing_then_it_is_an_error() {
    let server = MockServer::start().await;
    let directory = tempfile::tempdir().unwrap();
    let occupied = directory.path().join("a-file");
    std::fs::write(&occupied, "").unwrap();
    let installer = RecordingInstaller::new(
        &Path::new(&occupied).join("staging"),
        InstallResult::Succeeds,
    );
    let offer = published_release(&server, PAYLOAD).await;

    let error = updater(&server, Some(installer as _))
        .install(&offer, |_progress| {})
        .await
        .unwrap_err();

    assert!(matches!(error, UpdateError::Io { .. }), "{error}");
}
