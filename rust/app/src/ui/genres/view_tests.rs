use std::sync::Arc;

use slint::{ComponentHandle, Model, SharedString};

use super::binding::{Binding, with_grid, with_table};
use crate::controller::Command;
use crate::demo::FakeServer;
use crate::list_stream::{ListRow, SongSource};
use crate::tests::offline_server::OFFLINE;
use crate::tests::session_window::SessionWindow;
use crate::ui::{AppState, AppWindow, CardRow, GenresState, Route, TrackItem};

const GENRE_ID: &str = "genre-0";
const GENRE_NAME: &str = "Rock";
const ALL_GENRES: usize = 8;

const ALBUMS: Binding<CardRow> = Binding {
    rows: |window| window.global::<GenresState>().get_albums(),
    loading: |window, is_loading| {
        window
            .global::<GenresState>()
            .set_is_albums_loading(is_loading)
    },
};

const TRACKS: Binding<TrackItem> = Binding {
    rows: |window| window.global::<GenresState>().get_tracks(),
    loading: |window, is_loading| {
        window
            .global::<GenresState>()
            .set_is_tracks_loading(is_loading)
    },
};

fn open() -> SessionWindow {
    SessionWindow::open(Arc::new(FakeServer::new(2_000)))
}

fn navigate(view: &SessionWindow, genre_id: &str) {
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Genres, SharedString::from(genre_id));
}

fn genres(view: &SessionWindow) -> GenresState<'_> {
    view.app.global::<GenresState>()
}

fn genre_names(view: &SessionWindow) -> Vec<String> {
    genres(view)
        .get_genres()
        .iter()
        .map(|row| row.name.to_string())
        .collect()
}

fn first_card(window: &AppWindow) -> Option<crate::ui::Card> {
    let row = window.global::<GenresState>().get_albums().row_data(0)?;
    row.cards.row_data(0)
}

fn is_genre_loaded(view: &SessionWindow) -> bool {
    let state = genres(view);
    !state.get_is_albums_loading()
        && !state.get_is_tracks_loading()
        && state.get_tracks().row_count() > 0
        && state
            .get_tracks()
            .row_data(0)
            .is_some_and(|item| item.track.is_loaded)
        && first_card(&view.app).is_some_and(|card| card.is_loaded)
}

fn open_genre(view: &SessionWindow) {
    navigate(view, "");
    assert!(view.pump(|| genres(view).get_genres().row_count() == ALL_GENRES));
    navigate(view, GENRE_ID);
    assert!(view.pump(|| is_genre_loaded(view)));
}

#[test]
fn given_the_genres_page_when_it_opens_then_it_lists_every_genre_with_counts() {
    let view = open();

    navigate(&view, "");

    assert!(view.pump(|| genres(&view).get_genres().row_count() == ALL_GENRES));
    let state = genres(&view);
    assert!(!state.get_is_genres_loading());
    let rock = state.get_genres().row_data(0).unwrap_or_default();
    assert_eq!(rock.name, GENRE_NAME);
    assert!(rock.albums.parse::<u32>().is_ok_and(|count| count > 0));
    assert!(rock.tracks.parse::<u32>().is_ok_and(|count| count > 0));
}

#[test]
fn given_listed_genres_when_the_filter_is_edited_then_only_matching_genres_show() {
    let view = open();
    navigate(&view, "");
    assert!(view.pump(|| genres(&view).get_genres().row_count() == ALL_GENRES));

    genres(&view).invoke_filter_edited("  ROCK ".into());
    assert_eq!(genre_names(&view), [GENRE_NAME]);

    genres(&view).invoke_filter_edited("".into());
    assert_eq!(genre_names(&view).len(), ALL_GENRES);
}

#[test]
fn given_a_genre_when_it_opens_then_its_title_albums_and_tracks_load() {
    let view = open();

    open_genre(&view);

    let state = genres(&view);
    assert_eq!(state.get_title(), GENRE_NAME);
    assert!(!state.get_album_count().is_empty());
    assert!(!state.get_track_count().is_empty());
    assert!(view.pump(|| first_card(&view.app).is_some_and(|card| card.has_cover)));
}

#[test]
fn given_a_genre_opened_directly_when_the_genres_are_unknown_then_they_are_fetched_for_the_title() {
    let view = open();

    navigate(&view, GENRE_ID);

    assert!(view.pump(|| genres(&view).get_title() == GENRE_NAME));
    assert!(view.pump(|| is_genre_loaded(&view)));
}

#[test]
fn given_an_open_genre_when_the_album_columns_change_then_the_cards_are_laid_out_again() {
    let view = open();
    open_genre(&view);
    let state = genres(&view);

    state.invoke_album_columns(1);
    let one_per_row = state.get_albums().row_count();
    state.invoke_album_columns(2);
    let two_per_row = state.get_albums().row_count();

    assert_eq!(two_per_row, one_per_row.div_ceil(2));
    assert_eq!(
        state
            .get_albums()
            .row_data(0)
            .map(|row| row.cards.row_count()),
        Some(2)
    );
}

#[test]
fn given_an_open_genre_when_a_track_is_played_then_it_plays_from_the_genre_list() {
    let view = open();
    open_genre(&view);
    view.drain_commands();
    let first_id = genres(&view)
        .get_tracks()
        .row_data(0)
        .map(|item| item.track.id.to_string());

    genres(&view).invoke_play_track(0);
    genres(&view).invoke_play_track(-1);

    match view.next_command() {
        Some(Command::PlayFromList {
            source: SongSource::Library(query),
            song_id,
            row_index,
        }) => {
            assert_eq!(query.genre_ids, [GENRE_ID]);
            assert_eq!(song_id, first_id);
            assert_eq!(row_index, ListRow(0));
        }
        _ => panic!("expected PlayFromList"),
    }
    assert!(view.next_command().is_none());
}

#[test]
fn given_an_offline_server_when_the_genres_page_opens_then_the_status_shows_the_failure() {
    let view = open();
    view.go_offline();

    navigate(&view, "");

    assert!(view.pump_until_status(&format!("Could not load genres: {OFFLINE}")));
    assert!(!genres(&view).get_is_genres_loading());
}

#[test]
fn given_an_offline_server_when_a_genre_opens_then_albums_and_tracks_stop_loading_with_an_error() {
    let view = open();
    view.go_offline();

    navigate(&view, GENRE_ID);

    assert!(view.pump(|| {
        let state = genres(&view);
        !state.get_is_albums_loading() && !state.get_is_tracks_loading()
    }));
    assert!(view.pump_until_status("Could not load"));
    assert_eq!(genres(&view).get_tracks().row_count(), 0);
}

#[test]
fn given_a_failing_track_page_when_it_arrives_then_the_status_shows_the_failure() {
    let view = open();
    navigate(&view, "");
    assert!(view.pump(|| genres(&view).get_genres().row_count() == ALL_GENRES));
    view.go_offline();

    navigate(&view, GENRE_ID);
    view.clear_status();
    genres(&view).get_tracks().row_data(0);

    assert!(view.pump_until_status(&format!("Could not load tracks: {OFFLINE}")));
}

#[test]
fn given_a_genre_card_when_its_favorite_is_toggled_then_it_flips_and_stays_on_success() {
    let view = open();
    open_genre(&view);
    let card_id = first_card(&view.app)
        .map(|card| card.id)
        .unwrap_or_default();
    let was_favorite = with_grid(&view.app, ALBUMS, |grid| grid.is_favorite(&card_id)).flatten();

    ALBUMS.toggle_favorite(&view.app, &view.context, &card_id);

    let flipped = was_favorite.map(|is_favorite| !is_favorite);
    let is_favorite = || with_grid(&view.app, ALBUMS, |grid| grid.is_favorite(&card_id)).flatten();
    assert_eq!(is_favorite(), flipped);
    assert!(view.pump(|| true));
    assert_eq!(is_favorite(), flipped);
}

#[test]
fn given_an_offline_server_when_a_card_favorite_is_toggled_then_it_flips_back_with_an_error() {
    let view = open();
    open_genre(&view);
    let card_id = first_card(&view.app)
        .map(|card| card.id)
        .unwrap_or_default();
    let was_favorite = with_grid(&view.app, ALBUMS, |grid| grid.is_favorite(&card_id)).flatten();
    view.go_offline();

    ALBUMS.toggle_favorite(&view.app, &view.context, &card_id);
    ALBUMS.toggle_favorite(&view.app, &view.context, "missing-card");

    assert!(view.pump_until_status(&format!("Could not update favorite: {OFFLINE}")));
    let is_favorite = with_grid(&view.app, ALBUMS, |grid| grid.is_favorite(&card_id)).flatten();
    assert_eq!(is_favorite, was_favorite);
}

#[test]
fn given_an_offline_server_when_a_track_favorite_is_toggled_then_it_flips_back_with_an_error() {
    let view = open();
    open_genre(&view);
    let was_favorite = with_table(&view.app, TRACKS, |table| table.favorite_at(0)).flatten();
    view.go_offline();

    TRACKS.toggle_favorite(&view.app, &view.context, 0);

    assert!(view.pump_until_status(&format!("Could not update favorite: {OFFLINE}")));
    let is_favorite = with_table(&view.app, TRACKS, |table| table.favorite_at(0)).flatten();
    assert_eq!(is_favorite, was_favorite);
}

#[test]
fn given_a_connected_server_when_a_track_favorite_is_toggled_then_it_stays_flipped() {
    let view = open();
    open_genre(&view);
    let was_favorite = with_table(&view.app, TRACKS, |table| table.favorite_at(0)).flatten();

    TRACKS.toggle_favorite(&view.app, &view.context, 0);
    assert!(view.pump(|| true));

    let is_favorite = with_table(&view.app, TRACKS, |table| table.favorite_at(0)).flatten();
    let flipped = was_favorite.map(|(song_id, is_favorite)| (song_id, !is_favorite));
    assert_eq!(is_favorite, flipped);
    assert!(view.status().is_empty());
}

#[test]
fn given_no_server_when_favorites_are_toggled_then_nothing_is_sent() {
    let view = open();
    open_genre(&view);
    let card_id = first_card(&view.app)
        .map(|card| card.id)
        .unwrap_or_default();
    view.disconnect();

    ALBUMS.toggle_favorite(&view.app, &view.context, &card_id);
    TRACKS.toggle_favorite(&view.app, &view.context, 0);
    TRACKS.toggle_favorite(&view.app, &view.context, usize::MAX);

    assert!(view.pump(|| true));
    assert!(view.status().is_empty());
}
