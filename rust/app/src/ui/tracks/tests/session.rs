use std::sync::Arc;

use slint::{ComponentHandle, Model, SharedString};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::list_stream::{Placement, SongSource};
use crate::tests::offline_server::OFFLINE;
use crate::tests::session_window::SessionWindow;
use crate::ui::{AppState, PlaylistPickerState, Route, TrackRow, TracksState};

const SONG_COUNT: usize = 600;
const RATING: i32 = 4;

fn tracks(view: &SessionWindow) -> TracksState<'_> {
    view.app.global::<TracksState>()
}

fn row(view: &SessionWindow, index: usize) -> TrackRow {
    tracks(view).get_rows().row_data(index).unwrap_or_default()
}

fn open_tracks() -> SessionWindow {
    let view = SessionWindow::open(Arc::new(FakeServer::new(SONG_COUNT)));
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Tracks, SharedString::default());
    assert!(view.pump(|| {
        usize::try_from(tracks(&view).get_total()).ok() == Some(SONG_COUNT)
            && row(&view, 0).is_loaded
    }));
    view.drain_commands();
    view
}

fn select_first_three(view: &SessionWindow) {
    tracks(view).invoke_select_row(0, false, false);
    tracks(view).invoke_select_row(2, false, true);
    assert_eq!(tracks(view).get_selected_count(), 3);
}

fn select_everything(view: &SessionWindow) {
    tracks(view).invoke_select_all();
    assert_eq!(
        usize::try_from(tracks(view).get_selected_count()).ok(),
        Some(SONG_COUNT)
    );
}

fn first_rows(view: &SessionWindow) -> [TrackRow; 3] {
    [0, 1, 2].map(|index| row(view, index))
}

#[test]
fn given_selected_loaded_songs_when_they_are_favorited_and_rated_then_their_rows_change() {
    let view = open_tracks();
    select_first_three(&view);

    tracks(&view).invoke_favorite_selected(true);
    assert!(view.pump(|| first_rows(&view).iter().all(|track| track.is_favorite)));
    tracks(&view).invoke_favorite_selected(false);
    assert!(view.pump(|| first_rows(&view).iter().all(|track| !track.is_favorite)));
    tracks(&view).invoke_rate_selected(RATING);

    assert!(view.pump(|| first_rows(&view).iter().all(|track| track.rating == RATING)));
    assert!(view.status().is_empty());
}

#[test]
fn given_selected_loaded_songs_when_they_are_played_then_they_are_sent_to_the_queue() {
    let view = open_tracks();
    select_first_three(&view);

    tracks(&view).invoke_play_selected();
    tracks(&view).invoke_queue_selected(true);

    assert!(matches!(
        view.next_command(),
        Some(Command::PlaySongs { songs, song_id: None }) if songs.len() == 3
    ));
    assert!(matches!(view.next_command(), Some(Command::AddNext(songs)) if songs.len() == 3));
}

#[test]
fn given_selected_loaded_songs_when_added_to_a_playlist_then_the_picker_opens_for_them() {
    let view = open_tracks();
    select_first_three(&view);

    tracks(&view).invoke_add_selected_to_playlist();

    let picker = view.app.global::<PlaylistPickerState>();
    assert!(view.pump(|| picker.get_is_open()));
    assert_eq!(picker.get_song_count(), 3);
}

#[test]
fn given_an_offline_server_when_selected_songs_are_favorited_then_the_status_shows_the_failure() {
    let view = open_tracks();
    select_first_three(&view);
    view.go_offline();

    tracks(&view).invoke_favorite_selected(true);

    assert!(view.pump_until_status(&format!("Could not update favorites: {OFFLINE}")));
}

#[test]
fn given_an_offline_server_when_selected_songs_are_rated_then_the_status_shows_the_failure() {
    let view = open_tracks();
    select_first_three(&view);
    let ratings = first_rows(&view).map(|track| track.rating);
    view.go_offline();

    tracks(&view).invoke_rate_selected(RATING);

    assert!(view.pump_until_status(&format!("Could not set the rating: {OFFLINE}")));
    assert_eq!(first_rows(&view).map(|track| track.rating), ratings);
}

#[test]
fn given_every_song_selected_when_they_are_favorited_and_rated_then_the_loaded_rows_change() {
    let view = open_tracks();
    select_everything(&view);

    tracks(&view).invoke_favorite_selected(true);
    assert!(view.pump(|| first_rows(&view).iter().all(|track| track.is_favorite)));
    tracks(&view).invoke_rate_selected(RATING);

    assert!(view.pump(|| first_rows(&view).iter().all(|track| track.rating == RATING)));
    assert!(view.status().is_empty());
}

#[test]
fn given_every_song_selected_when_queued_last_then_the_whole_list_is_enqueued() {
    let view = open_tracks();
    select_everything(&view);

    tracks(&view).invoke_queue_selected(false);

    match view.next_command() {
        Some(Command::EnqueueFromList {
            source: SongSource::Library(_),
            placement,
            ..
        }) => assert_eq!(placement, Placement::Last),
        _ => panic!("expected EnqueueFromList"),
    }
}

#[test]
fn given_every_song_selected_when_added_to_a_playlist_then_the_picker_opens_for_all_of_them() {
    let view = open_tracks();
    select_everything(&view);

    tracks(&view).invoke_add_selected_to_playlist();

    let picker = view.app.global::<PlaylistPickerState>();
    assert!(view.pump(|| picker.get_is_open()));
    assert_eq!(
        usize::try_from(picker.get_song_count()).ok(),
        Some(SONG_COUNT)
    );
}

#[test]
fn given_every_song_selected_and_an_offline_server_when_favorited_then_the_status_shows_the_failure()
 {
    let view = open_tracks();
    select_everything(&view);
    view.go_offline();

    tracks(&view).invoke_favorite_selected(true);

    assert!(view.pump_until_status(OFFLINE));
    assert!(first_rows(&view).iter().all(|track| !track.is_favorite));
}

#[test]
fn given_every_song_selected_and_an_offline_server_when_rated_then_the_status_shows_the_failure() {
    let view = open_tracks();
    select_everything(&view);
    view.go_offline();

    tracks(&view).invoke_rate_selected(RATING);

    assert!(view.pump_until_status(OFFLINE));
}

#[test]
fn given_a_selected_song_when_going_to_its_artist_then_the_artist_page_opens() {
    let view = open_tracks();
    let artist_id = row(&view, 1).artist_id;
    tracks(&view).invoke_select_row(1, false, false);

    tracks(&view).invoke_go_to_artist();

    let navigation = view.app.global::<AppState>();
    assert_eq!(
        (navigation.get_route(), navigation.get_detail_id()),
        (Route::ArtistDetail, artist_id)
    );
}

#[test]
fn given_an_unloaded_selected_song_when_going_to_its_album_then_it_is_fetched_first() {
    let view = open_tracks();
    let last = i32::try_from(SONG_COUNT - 1).unwrap_or_default();
    tracks(&view).invoke_select_row(last, false, false);
    assert_eq!(tracks(&view).get_selected_count(), 1);

    tracks(&view).invoke_go_to_album();

    let navigation = view.app.global::<AppState>();
    assert!(view.pump(|| navigation.get_route() == Route::AlbumDetail));
    assert!(!navigation.get_detail_id().is_empty());
}

#[test]
fn given_an_unloaded_selected_song_and_an_offline_server_when_going_to_its_album_then_the_status_shows_the_failure()
 {
    let view = open_tracks();
    let last = i32::try_from(SONG_COUNT - 1).unwrap_or_default();
    tracks(&view).invoke_select_row(last, false, false);
    view.go_offline();

    tracks(&view).invoke_go_to_album();

    assert!(view.pump_until_status(&format!("Could not load the selection: {OFFLINE}")));
    assert_eq!(view.app.global::<AppState>().get_route(), Route::Tracks);
}

#[test]
fn given_a_loaded_row_when_it_is_rated_then_the_row_shows_the_rating() {
    let view = open_tracks();

    tracks(&view).invoke_rate_row(0, RATING);
    tracks(&view).invoke_rate_row(-1, RATING);

    assert!(view.pump(|| row(&view, 0).rating == RATING));
}

#[test]
fn given_an_offline_server_when_a_row_favorite_is_toggled_then_the_status_shows_the_failure() {
    let view = open_tracks();
    let is_favorite = row(&view, 0).is_favorite;
    view.go_offline();

    tracks(&view).invoke_toggle_favorite(0);
    tracks(&view).invoke_toggle_favorite(-1);

    assert!(view.pump_until_status(&format!("Could not update favorites: {OFFLINE}")));
    assert_eq!(row(&view, 0).is_favorite, is_favorite);
}

#[test]
fn given_a_selection_when_it_is_cleared_then_nothing_is_selected() {
    let view = open_tracks();
    select_first_three(&view);

    tracks(&view).invoke_clear_selection();
    tracks(&view).invoke_context_row(-1);
    tracks(&view).invoke_select_row(-1, false, false);

    assert_eq!(tracks(&view).get_selected_count(), 0);
    tracks(&view).invoke_play_selected();
    assert!(view.next_command().is_none());
}

#[test]
fn given_a_search_text_when_the_debounce_ends_then_the_list_reloads_with_matching_songs() {
    let view = open_tracks();

    tracks(&view).invoke_search_edited("zzzz-no-match".into());

    assert!(view.pump(|| tracks(&view).get_total() == 0 && !tracks(&view).get_is_loading()));
}

#[test]
fn given_debounced_filters_when_the_debounce_ends_then_the_list_reloads() {
    let view = open_tracks();
    tracks(&view).set_is_favorite_filter_on(true);

    tracks(&view).invoke_filters_changed(true);

    assert!(view.pump(|| {
        let total = usize::try_from(tracks(&view).get_total()).unwrap_or(SONG_COUNT);
        total < SONG_COUNT && !tracks(&view).get_is_loading()
    }));
}
