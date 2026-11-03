#[cfg(not(windows))]
use std::process::{Command, Stdio};

use cadence_core::web_url::is_web_url;

use crate::{PlatformError, Result};

#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(any(target_os = "macos", windows)))]
const OPENER: &str = "xdg-open";
#[cfg(windows)]
const HIGHEST_SHELL_EXECUTE_ERROR: usize = 32;

pub fn open_url(url: &str) -> Result<()> {
    if !is_web_url(url) {
        return Err(PlatformError::OpenUrl(
            format!("{url} is not a web link").into(),
        ));
    }
    launch(url)
}

#[cfg(not(windows))]
fn launch(url: &str) -> Result<()> {
    run_opener(OPENER, url)
}

#[cfg(windows)]
fn launch(url: &str) -> Result<()> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, PCWSTR, w};
    let instance = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = instance.0.addr();
    match code > HIGHEST_SHELL_EXECUTE_ERROR {
        true => Ok(()),
        false => Err(PlatformError::OpenUrl(
            format!("ShellExecute failed with {code}").into(),
        )),
    }
}

#[cfg(not(windows))]
fn run_opener(opener: &str, url: &str) -> Result<()> {
    let status = Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| PlatformError::OpenUrl(Box::new(error)))?;
    match status.success() {
        true => Ok(()),
        false => Err(PlatformError::OpenUrl(
            format!("{opener} exited with {status}").into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_link_that_is_not_on_the_web_it_is_refused() {
        for url in [
            "file:///etc/hosts",
            "-a Calculator",
            "javascript:void(0)",
            "https://",
        ] {
            assert!(
                matches!(open_url(url), Err(PlatformError::OpenUrl(_))),
                "{url}"
            );
        }
    }

    #[cfg(not(windows))]
    const LINK: &str = "https://navidrome.example";

    #[cfg(not(windows))]
    #[test]
    fn given_the_opener_succeeds_when_opening_a_link_then_it_is_opened() {
        assert!(run_opener("true", LINK).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn given_the_opener_fails_when_opening_a_link_then_its_exit_status_is_the_error() {
        let error = run_opener("false", LINK).unwrap_err();
        assert!(matches!(error, PlatformError::OpenUrl(_)));
        assert!(error.to_string().contains("false exited with"), "{error}");
    }

    #[cfg(not(windows))]
    #[test]
    fn given_no_opener_is_installed_when_opening_a_link_then_the_launch_failure_is_the_error() {
        let error = run_opener("cadence-missing-link-opener", LINK).unwrap_err();
        let PlatformError::OpenUrl(cause) = error else {
            panic!("expected an open link error, got {error}");
        };
        assert!(cause.downcast_ref::<std::io::Error>().is_some());
    }

    #[test]
    fn given_an_http_or_https_link_it_is_a_web_link() {
        assert!(is_web_url("https://music.youtube.example/watch?v=video-1"));
        assert!(is_web_url("http://navidrome.example"));
        assert!(is_web_url("HTTPS://navidrome.example"));
        assert!(!is_web_url("httpsx://navidrome.example"));
    }
}
