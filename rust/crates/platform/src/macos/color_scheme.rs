use std::time::Duration;

use objc2_foundation::{NSString, NSUserDefaults};

use crate::{Result, SystemColorScheme};

const INTERFACE_STYLE_KEY: &str = "AppleInterfaceStyle";
const DARK_STYLE: &str = "Dark";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub async fn current_color_scheme() -> Result<SystemColorScheme> {
    Ok(read_color_scheme())
}

pub async fn watch_color_scheme(mut on_change: impl FnMut(SystemColorScheme)) -> Result<()> {
    let mut shown = read_color_scheme();
    on_change(shown);
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let current = read_color_scheme();
        if current != shown {
            shown = current;
            on_change(current);
        }
    }
}

fn read_color_scheme() -> SystemColorScheme {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str(INTERFACE_STYLE_KEY);
    let style = defaults.stringForKey(&key).map(|style| style.to_string());
    scheme_from_interface_style(style.as_deref())
}

fn scheme_from_interface_style(style: Option<&str>) -> SystemColorScheme {
    match style {
        Some(DARK_STYLE) => SystemColorScheme::Dark,
        _ => SystemColorScheme::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATCHED_POLLS: u32 = 3;

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
    }

    #[test]
    fn given_this_mac_when_watching_the_scheme_then_the_current_scheme_is_reported_first() {
        let runtime = paused_runtime();
        let current = runtime.block_on(current_color_scheme()).unwrap();
        let mut reported = Vec::new();
        let watching = runtime.block_on(async {
            tokio::time::timeout(
                POLL_INTERVAL * WATCHED_POLLS,
                watch_color_scheme(|scheme| reported.push(scheme)),
            )
            .await
        });
        assert!(watching.is_err());
        assert_eq!(reported.first(), Some(&current));
    }

    #[test]
    fn only_the_dark_style_is_dark() {
        assert_eq!(
            scheme_from_interface_style(Some("Dark")),
            SystemColorScheme::Dark
        );
        assert_eq!(scheme_from_interface_style(None), SystemColorScheme::Light);
    }
}
