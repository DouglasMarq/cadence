use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cadence_platform::{MediaControlEvent, PlatformError};
use cadence_updater::{Installer, Platform, Relaunch, ReleaseFeed, Result, Updater, Version};
use serde_json::json;
use slint::ComponentHandle;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::ui::test_window::TestWindow;
use crate::ui::updates::{OpenPage, PendingRelaunch, UpdateFlow, offer_if_available};
use crate::ui::{UpdateStage, UpdateState};

const ASSET: &str = "Cadence-2.0.0-x86_64.AppImage";
const CHECKSUMS: &str = "SHA256SUMS";
const PAGE: &str = "https://releases.example/v2.0.0";
const PAYLOAD: &[u8] = b"the new version";
const PAYLOAD_DIGEST: &str = "b455aa8ff3411d00febd8410453b884348a440f8c63249c5d012779cdbaf6677";
const OTHER_DIGEST: &str = "4b21c3d3b4db1ed6a1fa3b3ce7a8b1c4a0e6ee7d8fbae55d9ae4b12c8aa05acd";
const QUIET_PERIOD: Duration = Duration::from_millis(300);

struct RecordingInstaller {
    staging: PathBuf,
    relaunch_marker: PathBuf,
    installs: Mutex<usize>,
}

impl Installer for RecordingInstaller {
    fn staging_directory(&self) -> &Path {
        &self.staging
    }

    fn install(&self, download: &Path, _version: &Version) -> Result<()> {
        assert!(download.exists());
        *self.installs.lock().unwrap() += 1;
        Ok(())
    }

    fn relaunch(&self) -> Relaunch {
        Relaunch::new("touch", [OsString::from(&self.relaunch_marker)])
    }
}

struct Harness {
    view: TestWindow,
    server: MockServer,
    installer: Arc<RecordingInstaller>,
    relaunch: PendingRelaunch,
    opened: Arc<Mutex<Vec<String>>>,
    _staging: TempDir,
}

impl Harness {
    fn open() -> Option<Self> {
        let view = TestWindow::open(Arc::new(FakeServer::new(10)))?;
        let server = view.context.runtime.block_on(MockServer::start());
        let staging = tempfile::tempdir().unwrap();
        let installer = Arc::new(RecordingInstaller {
            staging: staging.path().join("staging"),
            relaunch_marker: staging.path().join("relaunched"),
            installs: Mutex::new(0),
        });
        Some(Self {
            view,
            server,
            installer,
            relaunch: PendingRelaunch::default(),
            opened: Arc::default(),
            _staging: staging,
        })
    }

    fn state(&self) -> UpdateState<'_> {
        self.view.app.global::<UpdateState>()
    }

    fn mount(&self, url_path: &str, response: ResponseTemplate) {
        let mock = Mock::given(method("GET"))
            .and(path(url_path))
            .respond_with(response);
        self.view.context.runtime.block_on(mock.mount(&self.server));
    }

    fn publish(&self, listed_digest: &str) {
        let url = |name: &str| format!("{}/download/{name}", self.server.uri());
        self.mount(
            "/latest",
            ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v2.0.0",
                "html_url": PAGE,
                "assets": [
                    { "name": ASSET, "browser_download_url": url(ASSET), "size": PAYLOAD.len() },
                    { "name": CHECKSUMS, "browser_download_url": url(CHECKSUMS), "size": 90 },
                ],
            })),
        );
        self.mount(
            &format!("/download/{ASSET}"),
            ResponseTemplate::new(200).set_body_bytes(PAYLOAD),
        );
        self.mount(
            &format!("/download/{CHECKSUMS}"),
            ResponseTemplate::new(200).set_body_string(format!("{listed_digest}  {ASSET}\n")),
        );
    }

    fn check(&self, can_install: bool, open_page: OpenPage) {
        let feed = ReleaseFeed::new(
            reqwest::Client::new(),
            format!("{}/latest", self.server.uri()),
        );
        let installer: Option<Arc<dyn Installer>> =
            can_install.then(|| Arc::clone(&self.installer) as _);
        let updater = Updater::new(
            feed,
            Platform::Linux("x86_64"),
            Version::new(1, 0, 0),
            installer,
        );
        let flow = UpdateFlow {
            context: self.view.context.clone(),
            updater: Arc::new(updater),
            open_page,
            relaunch: self.relaunch.clone(),
        };
        let checking = offer_if_available(self.view.app.as_weak(), flow);
        let task = self.view.context.runtime.spawn(checking);
        assert!(self.view.pump(|| task.is_finished()));
        assert!(self.view.pump(|| true));
    }

    fn recording_opener(&self) -> OpenPage {
        let opened = Arc::clone(&self.opened);
        Arc::new(move |url| {
            opened.lock().unwrap().push(url.to_owned());
            Ok(())
        })
    }

    fn offer(&self, can_install: bool) {
        self.publish(PAYLOAD_DIGEST);
        self.check(can_install, self.recording_opener());
        assert_eq!(self.state().get_stage(), UpdateStage::Offered);
    }

    fn has_requested_quit(&self) -> bool {
        std::iter::from_fn(|| self.view.next_command())
            .any(|command| matches!(command, Command::Media(MediaControlEvent::Quit)))
    }

    fn installs(&self) -> usize {
        *self.installer.installs.lock().unwrap()
    }

    fn download_requests(&self) -> usize {
        let requests = self
            .view
            .context
            .runtime
            .block_on(self.server.received_requests())
            .unwrap();
        requests
            .iter()
            .filter(|request| request.url.path().ends_with(ASSET))
            .count()
    }
}

#[test]
fn given_a_newer_release_that_can_be_installed_when_checking_then_the_prompt_offers_it() {
    let Some(harness) = Harness::open() else {
        return;
    };

    harness.offer(true);

    assert_eq!(harness.state().get_version(), "2.0.0");
    assert!(harness.state().get_can_install());
}

#[test]
fn given_an_offer_when_nobody_answers_then_the_app_never_restarts_on_its_own() {
    let Some(harness) = Harness::open() else {
        return;
    };

    harness.offer(true);

    assert!(!harness.has_requested_quit());
    assert_eq!(harness.download_requests(), 0);
    assert_eq!(harness.installs(), 0);
}

#[test]
fn given_no_release_when_checking_then_the_user_is_not_told_anything() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.mount("/latest", ResponseTemplate::new(404));

    harness.check(true, harness.recording_opener());

    assert_eq!(harness.state().get_stage(), UpdateStage::Hidden);
    assert_eq!(harness.view.status(), "");
}

#[test]
fn given_the_rate_limit_is_exhausted_when_checking_then_the_user_is_not_told_anything() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.mount("/latest", ResponseTemplate::new(403));

    harness.check(true, harness.recording_opener());

    assert_eq!(harness.state().get_stage(), UpdateStage::Hidden);
    assert_eq!(harness.view.status(), "");
}

#[test]
fn given_an_offer_when_choosing_later_then_the_prompt_closes_and_nothing_is_downloaded() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(true);

    harness.state().invoke_later();

    assert!(harness.view.pump(|| true));
    assert_eq!(harness.state().get_stage(), UpdateStage::Hidden);
    assert_eq!(harness.download_requests(), 0);
    assert!(!harness.has_requested_quit());
}

#[test]
fn given_an_offer_when_accepting_then_it_installs_and_quits_through_the_saving_path() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(true);

    harness.state().invoke_update_and_restart();

    assert_eq!(harness.state().get_stage(), UpdateStage::Downloading);
    assert!(harness.view.pump(|| harness.installs() == 1));
    assert!(harness.view.pump(|| harness.has_requested_quit()));
    assert_eq!(harness.view.status(), "");
}

#[test]
fn given_an_accepted_update_when_the_old_process_has_exited_then_the_new_version_is_started() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(true);
    let mut old_process = Process::new("sleep").arg("60").spawn().unwrap();
    let old_process_id = old_process.id();
    old_process.kill().unwrap();
    old_process.wait().unwrap();

    harness.state().invoke_update_and_restart();
    assert!(harness.view.pump(|| harness.has_requested_quit()));
    harness.relaunch.run_after_exit_of(old_process_id);

    assert!(
        harness
            .view
            .pump(|| harness.installer.relaunch_marker.exists())
    );
}

#[test]
fn given_nothing_was_accepted_when_the_app_exits_then_nothing_is_started() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(true);
    let mut old_process = Process::new("sleep").arg("60").spawn().unwrap();
    let old_process_id = old_process.id();
    old_process.kill().unwrap();
    old_process.wait().unwrap();

    harness.state().invoke_later();
    harness.relaunch.run_after_exit_of(old_process_id);

    std::thread::sleep(QUIET_PERIOD);
    assert!(!harness.installer.relaunch_marker.exists());
}

#[test]
fn given_an_offer_when_installing_then_the_stage_ends_on_installing() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(true);

    harness.state().invoke_update_and_restart();

    let state = harness.state();
    assert!(
        harness
            .view
            .pump(|| state.get_stage() == UpdateStage::Installing)
    );
    assert_eq!(state.get_percent(), 100);
}

#[test]
fn given_a_corrupted_release_when_accepting_then_the_failure_is_shown_and_the_app_keeps_running() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.publish(OTHER_DIGEST);
    harness.check(true, harness.recording_opener());
    assert_eq!(harness.state().get_stage(), UpdateStage::Offered);

    harness.state().invoke_update_and_restart();

    assert!(harness.view.pump(|| !harness.view.status().is_empty()));
    assert_eq!(
        harness.view.status(),
        format!("Update failed: the checksum of {ASSET} does not match")
    );
    assert_eq!(harness.state().get_stage(), UpdateStage::Hidden);
    assert_eq!(harness.installs(), 0);
    assert!(!harness.has_requested_quit());
}

#[test]
fn given_an_install_that_cannot_update_itself_when_offered_then_it_links_to_the_release_page() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.offer(false);

    assert!(!harness.state().get_can_install());
    harness.state().invoke_open_release_page();

    assert!(
        harness
            .view
            .pump(|| !harness.opened.lock().unwrap().is_empty())
    );
    assert_eq!(*harness.opened.lock().unwrap(), [PAGE.to_owned()]);
    assert_eq!(harness.state().get_stage(), UpdateStage::Hidden);
    assert_eq!(harness.download_requests(), 0);
    assert!(!harness.has_requested_quit());
}

#[test]
fn given_the_browser_cannot_be_opened_when_opening_the_page_then_the_failure_is_shown() {
    let Some(harness) = Harness::open() else {
        return;
    };
    harness.publish(PAYLOAD_DIGEST);
    let failing_opener: OpenPage =
        Arc::new(|_url| Err(PlatformError::OpenUrl("xdg-open exited with 1".into())));
    harness.check(false, failing_opener);

    harness.state().invoke_open_release_page();

    assert!(harness.view.pump(|| !harness.view.status().is_empty()));
    assert!(
        harness
            .view
            .status()
            .starts_with("Could not open the release page:"),
        "{}",
        harness.view.status()
    );
}
