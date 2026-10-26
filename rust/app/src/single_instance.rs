use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;

use cadence_storage::{APP_DIR_NAME, Paths};

const LOCK_FILE: &str = "instance.lock";
const SOCKET_FILE: &str = "instance.sock";
const RUNTIME_DIR_VARIABLE: &str = "XDG_RUNTIME_DIR";

pub enum Launch {
    First(FirstInstance),
    AlreadyRunning,
}

pub struct FirstInstance {
    _lock: File,
    listener: UnixListener,
}

impl FirstInstance {
    pub fn on_later_launch(self, on_launch: impl Fn() + Send + 'static) -> io::Result<()> {
        thread::Builder::new()
            .name("cadence-instance".into())
            .spawn(move || self.accept_launches(on_launch))?;
        Ok(())
    }

    fn accept_launches(self, on_launch: impl Fn()) {
        for connection in self.listener.incoming() {
            match connection {
                Ok(_) => on_launch(),
                Err(error) => tracing::debug!("accepting a later launch failed: {error}"),
            }
        }
    }
}

pub fn instance_directory() -> cadence_storage::Result<PathBuf> {
    match std::env::var_os(RUNTIME_DIR_VARIABLE).filter(|runtime_dir| !runtime_dir.is_empty()) {
        Some(runtime_dir) => Ok(PathBuf::from(runtime_dir).join(APP_DIR_NAME)),
        None => Ok(Paths::from_env()?.data_dir),
    }
}

pub fn claim(directory: &Path) -> io::Result<Launch> {
    fs::create_dir_all(directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(LOCK_FILE))?;
    let socket = directory.join(SOCKET_FILE);
    match lock.try_lock() {
        Ok(()) => listen(lock, &socket).map(Launch::First),
        Err(TryLockError::WouldBlock) => {
            notify_first_instance(&socket);
            Ok(Launch::AlreadyRunning)
        }
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn listen(lock: File, socket: &Path) -> io::Result<FirstInstance> {
    remove_stale_socket(socket)?;
    Ok(FirstInstance {
        _lock: lock,
        listener: UnixListener::bind(socket)?,
    })
}

fn remove_stale_socket(socket: &Path) -> io::Result<()> {
    match fs::remove_file(socket) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn notify_first_instance(socket: &Path) {
    if let Err(error) = UnixStream::connect(socket) {
        tracing::warn!("asking the running instance to show its window failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    const LAUNCH_TIMEOUT: Duration = Duration::from_secs(5);

    fn first(directory: &Path) -> FirstInstance {
        match claim(directory).unwrap() {
            Launch::First(first) => first,
            Launch::AlreadyRunning => panic!("expected the first instance"),
        }
    }

    #[test]
    fn given_no_running_instance_when_launching_then_it_is_the_first() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(claim(directory.path()), Ok(Launch::First(_))));
    }

    #[test]
    fn given_a_running_instance_when_launching_again_then_the_first_is_told() {
        let directory = tempfile::tempdir().unwrap();
        let (launched, launches) = mpsc::channel();
        first(directory.path())
            .on_later_launch(move || launched.send(()).unwrap())
            .unwrap();

        let second = claim(directory.path()).unwrap();

        assert!(matches!(second, Launch::AlreadyRunning));
        assert_eq!(launches.recv_timeout(LAUNCH_TIMEOUT), Ok(()));
    }

    #[test]
    fn given_the_first_instance_has_quit_when_launching_then_it_is_the_first_again() {
        let directory = tempfile::tempdir().unwrap();
        drop(first(directory.path()));

        assert!(matches!(claim(directory.path()), Ok(Launch::First(_))));
    }

    #[test]
    fn given_the_user_environment_the_instance_directory_belongs_to_the_app() {
        let directory = instance_directory().unwrap();
        assert!(directory.ends_with(APP_DIR_NAME));
    }

    #[test]
    fn given_a_missing_directory_when_launching_then_it_is_created() {
        let parent = tempfile::tempdir().unwrap();
        let directory = parent.path().join("runtime").join(APP_DIR_NAME);

        assert!(matches!(claim(&directory), Ok(Launch::First(_))));
        assert!(directory.join(SOCKET_FILE).exists());
    }
}
