#![cfg(test)]

#[cfg(unix)]
mod app_image;
mod check;
mod install;
#[cfg(target_os = "macos")]
mod mac_bundle;
#[cfg(windows)]
mod portable_folder;
mod relaunch;
mod support;
