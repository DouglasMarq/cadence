use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{Result, StorageError};

pub const APP_DIR_NAME: &str = "cadence";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BaseDirectory {
    variable: &'static str,
    subdirectory: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HomeLayout {
    pub(crate) config: BaseDirectory,
    cache: BaseDirectory,
    data: BaseDirectory,
}

#[cfg(any(all(test, unix), not(any(target_os = "macos", windows))))]
const FREEDESKTOP_LAYOUT: HomeLayout = HomeLayout {
    config: BaseDirectory {
        variable: "HOME",
        subdirectory: ".config",
    },
    cache: BaseDirectory {
        variable: "HOME",
        subdirectory: ".cache",
    },
    data: BaseDirectory {
        variable: "HOME",
        subdirectory: ".local/share",
    },
};

#[cfg(any(all(test, unix), target_os = "macos"))]
const MACOS_LAYOUT: HomeLayout = HomeLayout {
    config: BaseDirectory {
        variable: "HOME",
        subdirectory: "Library/Application Support",
    },
    cache: BaseDirectory {
        variable: "HOME",
        subdirectory: "Library/Caches",
    },
    data: BaseDirectory {
        variable: "HOME",
        subdirectory: "Library/Application Support",
    },
};

#[cfg(any(test, windows))]
const WINDOWS_LAYOUT: HomeLayout = HomeLayout {
    config: BaseDirectory {
        variable: "APPDATA",
        subdirectory: "",
    },
    cache: BaseDirectory {
        variable: "LOCALAPPDATA",
        subdirectory: "",
    },
    data: BaseDirectory {
        variable: "APPDATA",
        subdirectory: "",
    },
};

#[cfg(target_os = "macos")]
pub(crate) const PLATFORM_LAYOUT: HomeLayout = MACOS_LAYOUT;

#[cfg(windows)]
pub(crate) const PLATFORM_LAYOUT: HomeLayout = WINDOWS_LAYOUT;

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) const PLATFORM_LAYOUT: HomeLayout = FREEDESKTOP_LAYOUT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|variable_name| std::env::var_os(variable_name))
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        Self::from_lookup_with_layout(lookup, PLATFORM_LAYOUT)
    }

    fn from_lookup_with_layout(
        lookup: impl Fn(&str) -> Option<OsString>,
        layout: HomeLayout,
    ) -> Result<Self> {
        Ok(Self {
            config_dir: xdg_base(&lookup, "XDG_CONFIG_HOME", layout.config)?.join(APP_DIR_NAME),
            cache_dir: xdg_base(&lookup, "XDG_CACHE_HOME", layout.cache)?.join(APP_DIR_NAME),
            data_dir: xdg_base(&lookup, "XDG_DATA_HOME", layout.data)?.join(APP_DIR_NAME),
        })
    }

    pub fn under(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            data_dir: root.join("data"),
        }
    }
}

fn xdg_base(
    lookup: &impl Fn(&str) -> Option<OsString>,
    variable_name: &str,
    fallback: BaseDirectory,
) -> Result<PathBuf> {
    match absolute_path_from(lookup, variable_name) {
        Some(base) => Ok(base),
        None => fallback_directory(lookup, variable_name, fallback),
    }
}

fn fallback_directory(
    lookup: &impl Fn(&str) -> Option<OsString>,
    variable_name: &str,
    fallback: BaseDirectory,
) -> Result<PathBuf> {
    absolute_path_from(lookup, fallback.variable)
        .map(|base| base.join(fallback.subdirectory))
        .ok_or_else(|| {
            StorageError::Invalid(format!(
                "neither {variable_name} nor {} is set",
                fallback.variable
            ))
        })
}

fn absolute_path_from(
    lookup: &impl Fn(&str) -> Option<OsString>,
    variable_name: &str,
) -> Option<PathBuf> {
    lookup(variable_name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup(variables: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let environment: HashMap<String, OsString> = variables
            .iter()
            .map(|(name, value)| (name.to_string(), OsString::from(value)))
            .collect();
        move |name| environment.get(name).cloned()
    }

    #[cfg(unix)]
    fn freedesktop(variables: &[(&str, &str)]) -> Result<Paths> {
        Paths::from_lookup_with_layout(lookup(variables), FREEDESKTOP_LAYOUT)
    }

    fn windows(variables: &[(&str, &Path)]) -> Result<Paths> {
        let variables: Vec<(&str, &str)> = variables
            .iter()
            .map(|(name, path)| (*name, path.to_str().unwrap()))
            .collect();
        Paths::from_lookup_with_layout(lookup(&variables), WINDOWS_LAYOUT)
    }

    #[cfg(unix)]
    #[test]
    fn home_fallbacks() {
        let paths = freedesktop(&[("HOME", "/home/u")]).unwrap();
        assert_eq!(paths.config_dir, PathBuf::from("/home/u/.config/cadence"));
        assert_eq!(paths.cache_dir, PathBuf::from("/home/u/.cache/cadence"));
        assert_eq!(
            paths.data_dir,
            PathBuf::from("/home/u/.local/share/cadence")
        );
    }

    #[cfg(unix)]
    #[test]
    fn xdg_vars_win() {
        let paths = freedesktop(&[
            ("HOME", "/home/u"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_CACHE_HOME", "/cache"),
            ("XDG_DATA_HOME", "/data"),
        ])
        .unwrap();
        assert_eq!(paths.config_dir, PathBuf::from("/cfg/cadence"));
        assert_eq!(paths.cache_dir, PathBuf::from("/cache/cadence"));
        assert_eq!(paths.data_dir, PathBuf::from("/data/cadence"));
    }

    #[cfg(unix)]
    #[test]
    fn relative_or_empty_xdg_is_ignored() {
        let paths = freedesktop(&[
            ("HOME", "/home/u"),
            ("XDG_CONFIG_HOME", "relative"),
            ("XDG_CACHE_HOME", ""),
        ])
        .unwrap();
        assert_eq!(paths.config_dir, PathBuf::from("/home/u/.config/cadence"));
        assert_eq!(paths.cache_dir, PathBuf::from("/home/u/.cache/cadence"));
    }

    #[cfg(unix)]
    #[test]
    fn macos_home_fallbacks_use_the_library_folder() {
        let paths =
            Paths::from_lookup_with_layout(lookup(&[("HOME", "/Users/u")]), MACOS_LAYOUT).unwrap();
        assert_eq!(
            paths.config_dir,
            PathBuf::from("/Users/u/Library/Application Support/cadence")
        );
        assert_eq!(
            paths.cache_dir,
            PathBuf::from("/Users/u/Library/Caches/cadence")
        );
        assert_eq!(
            paths.data_dir,
            PathBuf::from("/Users/u/Library/Application Support/cadence")
        );
    }

    #[test]
    fn given_the_windows_layout_when_resolving_then_config_and_data_roam_and_the_cache_stays_local()
    {
        let roaming = std::env::temp_dir().join("Roaming");
        let local = std::env::temp_dir().join("Local");
        let paths = windows(&[("APPDATA", &roaming), ("LOCALAPPDATA", &local)]).unwrap();
        assert_eq!(paths.config_dir, roaming.join(APP_DIR_NAME));
        assert_eq!(paths.cache_dir, local.join(APP_DIR_NAME));
        assert_eq!(paths.data_dir, roaming.join(APP_DIR_NAME));
    }

    #[test]
    fn given_the_windows_layout_when_xdg_variables_are_set_then_they_win() {
        let roaming = std::env::temp_dir().join("Roaming");
        let config = std::env::temp_dir().join("config");
        let paths = windows(&[
            ("APPDATA", &roaming),
            ("LOCALAPPDATA", &roaming),
            ("XDG_CONFIG_HOME", &config),
        ])
        .unwrap();
        assert_eq!(paths.config_dir, config.join(APP_DIR_NAME));
        assert_eq!(paths.data_dir, roaming.join(APP_DIR_NAME));
    }

    #[test]
    fn given_the_windows_layout_when_local_app_data_is_missing_then_it_is_named_in_the_error() {
        let roaming = std::env::temp_dir().join("Roaming");
        let error = windows(&[("APPDATA", &roaming)]).unwrap_err();
        assert!(error.to_string().contains("LOCALAPPDATA"), "{error}");
    }

    #[test]
    fn missing_home_errors() {
        assert!(matches!(
            Paths::from_lookup(lookup(&[])),
            Err(StorageError::Invalid(_))
        ));
        let paths = Paths::from_lookup(lookup(&[
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_CACHE_HOME", "/k"),
        ]));
        assert!(paths.is_err());
    }

    #[test]
    fn from_env_does_not_fail_with_home() {
        if std::env::var_os("HOME").is_some() {
            let paths = Paths::from_env().unwrap();
            assert!(paths.config_dir.ends_with(APP_DIR_NAME));
        }
    }
}
