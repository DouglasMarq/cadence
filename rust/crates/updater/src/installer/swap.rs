use std::io;
use std::path::{Path, PathBuf};

use crate::error::{Result, UpdateError, io_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Swap {
    pub(super) installed: PathBuf,
    pub(super) staged: PathBuf,
    pub(super) backup: PathBuf,
}

pub(super) fn put_in_place(
    swap: &Swap,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    rename(&swap.installed, &swap.backup).map_err(io_error("moving the installed app aside"))?;
    let Err(replace_error) = rename(&swap.staged, &swap.installed) else {
        return Ok(());
    };
    match rename(&swap.backup, &swap.installed) {
        Ok(()) => Err(io_error("putting the new app in place")(replace_error)),
        Err(source) => Err(UpdateError::RollbackFailed {
            backup: swap.backup.clone(),
            source,
        }),
    }
}

#[cfg(any(windows, test))]
pub(super) fn put_all_in_place(
    swaps: &[Swap],
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    for (swapped_count, swap) in swaps.iter().enumerate() {
        if let Err(error) = put_in_place(swap, &rename) {
            put_back(swaps.iter().take(swapped_count).rev(), &rename)?;
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn put_back<'swaps>(
    swapped: impl Iterator<Item = &'swaps Swap>,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    for swap in swapped {
        rename(&swap.installed, &swap.staged)
            .and_then(|()| rename(&swap.backup, &swap.installed))
            .map_err(|source| UpdateError::RollbackFailed {
                backup: swap.backup.clone(),
                source,
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    struct Versions {
        _directory: TempDir,
        swap: Swap,
    }

    impl Versions {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let swap = Swap {
                installed: directory.path().join("installed"),
                staged: directory.path().join("staged"),
                backup: directory.path().join("backup"),
            };
            fs::write(&swap.installed, "1.0.0").unwrap();
            fs::write(&swap.staged, "2.0.0").unwrap();
            Self {
                _directory: directory,
                swap,
            }
        }
    }

    fn version_at(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn real_rename(from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    #[test]
    fn given_a_staged_version_when_putting_it_in_place_then_it_replaces_the_installed_one() {
        let versions = Versions::new();

        put_in_place(&versions.swap, real_rename).unwrap();

        assert_eq!(version_at(&versions.swap.installed), "2.0.0");
        assert_eq!(version_at(&versions.swap.backup), "1.0.0");
        assert!(!versions.swap.staged.exists());
    }

    #[test]
    fn given_the_new_version_cannot_be_moved_when_putting_it_in_place_then_the_old_one_is_put_back()
    {
        let versions = Versions::new();
        fs::remove_file(&versions.swap.staged).unwrap();

        let error = put_in_place(&versions.swap, real_rename).unwrap_err();

        assert!(matches!(error, UpdateError::Io { .. }), "{error}");
        assert_eq!(version_at(&versions.swap.installed), "1.0.0");
        assert!(!versions.swap.backup.exists());
    }

    #[test]
    fn given_the_installed_version_cannot_be_moved_when_putting_it_in_place_then_nothing_changes() {
        let versions = Versions::new();
        let swap = Swap {
            backup: versions.swap.backup.join("missing").join("backup"),
            ..versions.swap.clone()
        };

        let error = put_in_place(&swap, real_rename).unwrap_err();

        assert!(matches!(error, UpdateError::Io { .. }), "{error}");
        assert_eq!(version_at(&swap.installed), "1.0.0");
        assert_eq!(version_at(&swap.staged), "2.0.0");
    }

    #[test]
    fn given_the_rollback_also_fails_when_putting_it_in_place_then_the_backup_location_is_reported()
    {
        let versions = Versions::new();
        let refuse_to_restore = |from: &Path, to: &Path| match to == versions.swap.installed {
            true => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            false => fs::rename(from, to),
        };

        let error = put_in_place(&versions.swap, refuse_to_restore).unwrap_err();

        let UpdateError::RollbackFailed { backup, .. } = error else {
            panic!("expected a failed rollback, got {error}");
        };
        assert_eq!(backup, versions.swap.backup);
        assert_eq!(version_at(&versions.swap.backup), "1.0.0");
    }

    struct Release {
        _directory: TempDir,
        swaps: [Swap; 2],
    }

    impl Release {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let swap_of = |name: &str| {
                let swap = Swap {
                    installed: directory.path().join(name),
                    staged: directory.path().join("extracted").join(name),
                    backup: directory.path().join("previous").join(name),
                };
                fs::create_dir_all(swap.staged.parent().unwrap()).unwrap();
                fs::create_dir_all(swap.backup.parent().unwrap()).unwrap();
                fs::write(&swap.installed, "1.0.0").unwrap();
                fs::write(&swap.staged, "2.0.0").unwrap();
                swap
            };
            let swaps = [swap_of("libmpv-2.dll"), swap_of("cadence.exe")];
            Self {
                _directory: directory,
                swaps,
            }
        }

        fn installed_versions(&self) -> Vec<String> {
            self.swaps
                .iter()
                .map(|swap| version_at(&swap.installed))
                .collect()
        }
    }

    #[test]
    fn given_every_file_of_a_release_when_putting_them_in_place_then_each_one_is_replaced() {
        let release = Release::new();

        put_all_in_place(&release.swaps, real_rename).unwrap();

        assert_eq!(release.installed_versions(), ["2.0.0", "2.0.0"]);
        assert!(
            release
                .swaps
                .iter()
                .all(|swap| version_at(&swap.backup) == "1.0.0")
        );
    }

    #[test]
    fn given_a_later_file_fails_when_putting_them_in_place_then_the_earlier_ones_are_put_back() {
        let release = Release::new();
        let [_, executable] = &release.swaps;
        fs::remove_file(&executable.staged).unwrap();

        let error = put_all_in_place(&release.swaps, real_rename).unwrap_err();

        assert!(matches!(error, UpdateError::Io { .. }), "{error}");
        assert_eq!(release.installed_versions(), ["1.0.0", "1.0.0"]);
        assert!(release.swaps.iter().all(|swap| !swap.backup.exists()));
    }

    #[test]
    fn given_an_earlier_file_cannot_be_put_back_when_a_later_one_fails_then_its_backup_is_reported()
    {
        let release = Release::new();
        let [library, executable] = &release.swaps;
        fs::remove_file(&executable.staged).unwrap();
        let refuse_to_restore_the_library = |from: &Path, to: &Path| match from == library.backup {
            true => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            false => fs::rename(from, to),
        };

        let error = put_all_in_place(&release.swaps, refuse_to_restore_the_library).unwrap_err();

        let UpdateError::RollbackFailed { backup, .. } = error else {
            panic!("expected a failed rollback, got {error}");
        };
        assert_eq!(backup, library.backup);
        assert_eq!(version_at(&library.backup), "1.0.0");
        assert_eq!(version_at(&executable.installed), "1.0.0");
    }
}
