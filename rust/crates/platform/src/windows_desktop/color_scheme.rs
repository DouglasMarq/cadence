use windows_registry::CURRENT_USER;

use crate::polled_color_scheme::watch_by_polling;
use crate::{Result, SystemColorScheme};

const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS_USE_LIGHT_THEME: &str = "AppsUseLightTheme";
const DARK_APPS: u32 = 0;
const LIGHT_APPS: u32 = 1;

pub async fn current_color_scheme() -> Result<SystemColorScheme> {
    Ok(read_color_scheme())
}

pub async fn watch_color_scheme(on_change: impl FnMut(SystemColorScheme)) -> Result<()> {
    watch_by_polling(read_color_scheme, on_change).await
}

fn read_color_scheme() -> SystemColorScheme {
    let setting = CURRENT_USER
        .open(PERSONALIZE_KEY)
        .and_then(|key| key.get_u32(APPS_USE_LIGHT_THEME))
        .inspect_err(|error| tracing::debug!("the Windows app theme is unreadable: {error}"));
    scheme_from_light_theme_setting(setting.ok())
}

fn scheme_from_light_theme_setting(setting: Option<u32>) -> SystemColorScheme {
    match setting {
        Some(DARK_APPS) => SystemColorScheme::Dark,
        Some(LIGHT_APPS) => SystemColorScheme::Light,
        _ => SystemColorScheme::NoPreference,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_app_theme_setting_when_reading_it_then_zero_is_dark_and_one_is_light() {
        assert_eq!(
            scheme_from_light_theme_setting(Some(0)),
            SystemColorScheme::Dark
        );
        assert_eq!(
            scheme_from_light_theme_setting(Some(1)),
            SystemColorScheme::Light
        );
        assert_eq!(
            scheme_from_light_theme_setting(None),
            SystemColorScheme::NoPreference
        );
    }

    #[test]
    fn given_this_computer_when_reading_the_scheme_then_it_is_answered() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert!(runtime.block_on(current_color_scheme()).is_ok());
    }
}
