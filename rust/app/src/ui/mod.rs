use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::SortOrder;
use cadence_core::unix_time::since_unix_epoch;
use cadence_remote::ServerKey;
use cadence_storage::settings::{Settings, UiSettings};
use slint::{ComponentHandle, Model, VecModel};
use tokio::sync::mpsc::UnboundedSender;

use crate::connect::Connector;
use crate::controller::Command;
use crate::covers::CoverProvider;
use crate::remote::{Remote, Routed};
use crate::session::Session;
use crate::store::Store;
use crate::views::{FocusedCovers, Views};

#[allow(clippy::all, clippy::pedantic, clippy::restriction, clippy::nursery)]
mod generated {
    slint::include_modules!();
}

pub use generated::*;

pub mod album_detail;
pub mod album_grid;
pub mod albums;
pub mod appearance;
pub mod artist_detail;
pub mod artists;
pub mod busy_generation;
pub mod cover_lru;
pub mod cover_slots;
pub mod devices;
pub mod discover;
pub mod favorites;
pub mod folders;
pub mod genres;
pub mod grid_pages;
pub mod home;
pub mod images;
pub mod library_scan;
pub mod login;
pub mod nav;
pub mod now_playing;
pub mod page_backoff;
pub mod page_cache;
pub mod player;
pub mod playlist_detail;
pub mod playlist_picker;
pub mod playlists;
pub mod queue;
pub mod radio;
pub mod rows;
pub mod search;
pub mod settings;
pub mod sharing;
pub mod tracks;

#[cfg(test)]
mod detail_navigation_tests;
#[cfg(test)]
mod devices_view_tests;
#[cfg(test)]
mod field_widgets_tests;
#[cfg(test)]
mod sharing_view_tests;
#[cfg(test)]
pub mod test_window;

pub const MAX_RATING: u8 = 5;

#[derive(Clone)]
pub struct Context {
    pub runtime: tokio::runtime::Handle,
    pub commands: UnboundedSender<Command>,
    pub session: Session,
    pub store: Arc<dyn Store>,
    pub connector: Arc<dyn Connector>,
    pub covers: Arc<dyn CoverProvider>,
    pub remote: Remote,
}

impl Context {
    pub fn send(&self, command: Command) {
        let local_server = self
            .session
            .get()
            .map(|server| ServerKey::of(server.config()));
        match self.remote.target.route(command, local_server.as_ref()) {
            Routed::Remote { target, control } => target.send(control),
            Routed::Local(command) => self.send_to_controller(command),
        }
    }

    fn send_to_controller(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::warn!("controller is gone; command dropped");
        }
    }

    pub fn views(&self, server: &Arc<dyn MusicServer>) -> Views {
        Views::new(
            Arc::clone(server),
            self.store.views(),
            self.session.clone(),
            self.runtime.clone(),
        )
    }

    pub fn view_covers(&self) -> Arc<dyn CoverProvider> {
        Arc::new(FocusedCovers::new(
            Arc::clone(&self.covers),
            self.session.clone(),
        ))
    }
}

pub fn on_ui(window: &slint::Weak<AppWindow>, update: impl FnOnce(&AppWindow) + Send + 'static) {
    let window = window.clone();
    let queued = slint::invoke_from_event_loop(move || {
        if let Some(window) = window.upgrade() {
            update(&window);
        }
    });
    if let Err(error) = queued {
        tracing::debug!("UI event loop is gone: {error}");
    }
}

pub fn install(window: &AppWindow, context: &Context, settings: &Settings) {
    let picker = playlist_picker::install(window, context);
    tracks::install(
        window,
        &tracks::SongActions {
            context: context.clone(),
            picker: picker.clone(),
        },
        &settings.ui,
    );
    queue::install(window, context);
    player::install(window, context);
    login::install(window, context);
    nav::install(window);
    library_scan::install(window, context);
    home::install(window, context);
    albums::install(window, context);
    album_detail::install(window, context);
    artists::install(window, context);
    artist_detail::install(window, context);
    genres::install(window, context);
    playlists::install(window, context);
    playlist_detail::install(window, context, &picker);
    favorites::install(window, context);
    folders::install(window, context);
    search::install(window, context);
    settings::install(window, context);
    sharing::install(window, context);
    now_playing::install(window, context);
    appearance::install(window, context, &settings.appearance);
    radio::install(window, context);
    devices::install(window, context);
    window
        .global::<AppState>()
        .set_is_queue_open(settings.ui.is_queue_panel_open);
}

pub fn ui_settings(window: &AppWindow, previous: &UiSettings) -> UiSettings {
    UiSettings {
        track_columns: tracks::columns(window),
        is_queue_panel_open: window.global::<AppState>().get_is_queue_open(),
        ..previous.clone()
    }
}

pub fn show_status(window: &AppWindow, message: &str) {
    window.global::<AppState>().set_status(message.into());
}

pub fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let (hours, minutes, seconds) = (
        total_seconds / 3600,
        total_seconds / 60 % 60,
        total_seconds % 60,
    );
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

pub fn clamp_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub struct ListControls<'a> {
    pub sort_index: i32,
    pub is_descending: bool,
    pub search: &'a str,
    pub is_favorites_only: bool,
}

pub fn sort_order(is_descending: bool) -> SortOrder {
    match is_descending {
        true => SortOrder::Desc,
        false => SortOrder::Asc,
    }
}

pub fn sort_at<Sort: Copy>(sorts: &[Sort], sort_index: i32, fallback: Sort) -> Sort {
    usize::try_from(sort_index)
        .ok()
        .and_then(|index| sorts.get(index))
        .copied()
        .unwrap_or(fallback)
}

pub fn normalized_search(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub fn update_first_row<Row: Clone + 'static>(
    rows: &VecModel<Row>,
    is_target: impl Fn(&Row) -> bool,
    change: impl FnOnce(&mut Row),
) {
    let Some(index) = rows.iter().position(|row| is_target(&row)) else {
        return;
    };
    if let Some(mut row) = rows.row_data(index) {
        change(&mut row);
        rows.set_row_data(index, row);
    }
}

pub fn time_based_id() -> String {
    let nanos = since_unix_epoch().as_nanos();
    format!("{nanos:x}")
}

pub fn slint_index(index: usize) -> i32 {
    i32::try_from(index).unwrap_or_default()
}

pub fn rating_stars(rating: i32) -> u8 {
    u8::try_from(rating.clamp(0, i32::from(MAX_RATING))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_format_as_minutes_and_seconds_with_optional_hours() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(61_000), "1:01");
        assert_eq!(format_duration(3_723_000), "1:02:03");
    }

    #[test]
    fn given_a_rating_outside_the_star_range_it_is_clamped_to_zero_or_five_stars() {
        assert_eq!(rating_stars(-1), 0);
        assert_eq!(rating_stars(3), 3);
        assert_eq!(rating_stars(9), 5);
    }

    #[test]
    fn given_a_sort_index_outside_the_sorts_when_it_is_looked_up_then_the_fallback_is_used() {
        let sorts = [SortOrder::Desc, SortOrder::Asc];
        assert_eq!(sort_at(&sorts, 1, SortOrder::Desc), SortOrder::Asc);
        assert_eq!(sort_at(&sorts, 2, SortOrder::Desc), SortOrder::Desc);
        assert_eq!(sort_at(&sorts, -1, SortOrder::Asc), SortOrder::Asc);
    }

    #[test]
    fn given_search_text_when_it_is_normalized_then_blank_text_is_no_search() {
        assert_eq!(normalized_search("   "), None);
        assert_eq!(normalized_search("  river "), Some("river".to_owned()));
    }

    #[test]
    fn given_rows_when_the_first_matching_row_is_updated_then_only_it_changes() {
        let rows = VecModel::from(vec![1, 2, 2]);
        update_first_row(&rows, |row| *row == 2, |row| *row = 7);
        update_first_row(&rows, |row| *row == 9, |row| *row = 0);
        assert_eq!(rows.iter().collect::<Vec<_>>(), [1, 7, 2]);
    }

    #[test]
    fn given_a_time_based_id_it_is_a_hexadecimal_timestamp() {
        let id = time_based_id();
        assert!(u128::from_str_radix(&id, 16).is_ok_and(|nanos| nanos > 0));
    }

    #[test]
    fn given_an_index_beyond_slint_range_it_falls_back_to_the_first_row() {
        assert_eq!(slint_index(7), 7);
        assert_eq!(slint_index(usize::MAX), 0);
    }
}
