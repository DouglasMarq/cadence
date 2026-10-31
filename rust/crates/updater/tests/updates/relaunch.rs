use std::ffi::OsString;
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use cadence_updater::Relaunch;

const QUIET_PERIOD: Duration = Duration::from_millis(500);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(5);
const GIVE_UP_TIMEOUT: Duration = Duration::from_secs(20);

fn running_process() -> Child {
    Command::new("sleep").arg("60").spawn().unwrap()
}

fn touch(marker: &Path) -> Relaunch {
    Relaunch::new("touch", [OsString::from(marker)])
}

fn appears_within(timeout: Duration, marker: &Path) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if marker.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn given_a_running_process_when_relaunching_then_the_app_starts_only_after_it_exits() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("launched");
    let mut process = running_process();

    touch(&marker).after_exit_of(process.id()).unwrap();

    assert!(!appears_within(QUIET_PERIOD, &marker));
    process.kill().unwrap();
    process.wait().unwrap();
    assert!(appears_within(LAUNCH_TIMEOUT, &marker));
}

#[test]
fn given_a_process_that_already_exited_when_relaunching_then_the_app_starts_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("launched");
    let mut process = running_process();
    let process_id = process.id();
    process.kill().unwrap();
    process.wait().unwrap();

    touch(&marker).after_exit_of(process_id).unwrap();

    assert!(appears_within(LAUNCH_TIMEOUT, &marker));
}

#[test]
fn given_a_path_with_spaces_when_relaunching_then_it_stays_one_argument() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("Cadence Beta launched");
    let mut process = running_process();
    let process_id = process.id();
    process.kill().unwrap();
    process.wait().unwrap();

    touch(&marker).after_exit_of(process_id).unwrap();

    assert!(appears_within(LAUNCH_TIMEOUT, &marker));
}

#[test]
fn given_a_process_nobody_reaps_when_relaunching_then_the_wait_gives_up() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("launched");
    let mut unreaped = running_process();
    unreaped.kill().unwrap();

    touch(&marker).after_exit_of(unreaped.id()).unwrap();

    assert!(appears_within(GIVE_UP_TIMEOUT, &marker));
    unreaped.wait().unwrap();
}
