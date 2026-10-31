#![cfg(test)]

mod app_image;
mod check;
mod install;
#[cfg(target_os = "macos")]
mod mac_bundle;
mod relaunch;
mod support;
