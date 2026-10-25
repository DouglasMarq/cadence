use cadence_core::model::SongSort;
use cadence_storage::settings::ColumnSetting;
use slint::ComponentHandle;

use super::super::columns;
use super::super::columns::apply_columns;
use crate::ui::{AppWindow, TrackColumn, TracksState};

const FIRST_WIDTH: f32 = 41.0;

fn open() -> AppWindow {
    i_slint_backend_testing::init_no_event_loop();
    AppWindow::new().unwrap()
}

fn saved_columns() -> Vec<ColumnSetting> {
    TrackColumn::ALL
        .into_iter()
        .zip(0u8..)
        .map(|(column, position)| ColumnSetting {
            id: column.id().to_owned(),
            width_pixels: FIRST_WIDTH + f32::from(position),
            is_visible: position % 2 == 0,
        })
        .collect()
}

#[test]
fn given_saved_columns_when_they_are_applied_then_reading_them_back_returns_the_same_layout() {
    let window = open();
    let saved = saved_columns();

    apply_columns(&window, &saved);

    let read_back = columns(&window);
    let widths: Vec<f32> = read_back.iter().map(|column| column.width_pixels).collect();
    let saved_widths: Vec<f32> = saved.iter().map(|column| column.width_pixels).collect();
    assert_eq!(widths, saved_widths);
    let hidden: Vec<&str> = read_back
        .iter()
        .filter(|column| !column.is_visible)
        .map(|column| column.id.as_str())
        .collect();
    assert_eq!(hidden, ["album", "plays", "favorite"]);
}

#[test]
fn given_unknown_or_zero_width_columns_when_they_are_applied_then_they_are_ignored() {
    let window = open();
    let state = window.global::<TracksState>();
    let title_width = state.get_w_title();
    let ignored = [
        ColumnSetting {
            id: "title".into(),
            width_pixels: 0.0,
            is_visible: false,
        },
        ColumnSetting {
            id: "lyrics".into(),
            width_pixels: 90.0,
            is_visible: true,
        },
    ];

    apply_columns(&window, &ignored);

    assert_eq!(state.get_w_title(), title_width);
    assert!(columns(&window).iter().all(|column| column.id != "lyrics"));
}

#[test]
fn given_the_title_and_index_columns_when_hidden_then_they_stay_shown() {
    let window = open();
    let hidden = ["index", "title"].map(|id| ColumnSetting {
        id: id.into(),
        width_pixels: FIRST_WIDTH,
        is_visible: false,
    });

    apply_columns(&window, &hidden);

    let shown = columns(&window);
    assert!(shown.iter().take(2).all(|column| column.is_visible));
}

#[test]
fn given_each_column_when_sorted_by_then_it_maps_to_its_song_sort() {
    let sorts: Vec<SongSort> = TrackColumn::ALL
        .into_iter()
        .map(TrackColumn::sort)
        .collect();

    assert_eq!(
        sorts,
        [
            SongSort::Name,
            SongSort::Name,
            SongSort::Artist,
            SongSort::Album,
            SongSort::Year,
            SongSort::PlayCount,
            SongSort::Duration,
            SongSort::Favorited,
            SongSort::Rating,
        ]
    );
    assert_eq!(TrackColumn::from_id("plays"), Some(TrackColumn::Plays));
    assert_eq!(TrackColumn::from_id("lyrics"), None);
}
