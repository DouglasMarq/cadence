use std::sync::Arc;

use slint::{ComponentHandle, Model, SharedString};

use super::query::SEARCH_LIMIT;
use crate::controller::Command;
use crate::demo::FakeServer;
use crate::tests::offline_server::OFFLINE;
use crate::tests::session_window::SessionWindow;
use crate::ui::{AppState, Route, SearchKind, SearchState};

const QUERY: &str = "e";

fn open() -> SessionWindow {
    let view = SessionWindow::open(Arc::new(FakeServer::new(2_000)));
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Search, SharedString::default());
    assert!(view.pump(|| true));
    view
}

fn search(view: &SessionWindow) -> SearchState<'_> {
    view.app.global::<SearchState>()
}

fn shown_counts(view: &SessionWindow) -> [usize; 3] {
    let state = search(view);
    [
        state.get_songs().row_count(),
        state.get_albums().row_count(),
        state.get_artists().row_count(),
    ]
}

fn is_idle(view: &SessionWindow) -> bool {
    let state = search(view);
    !state.get_is_loading()
        && !state.get_is_songs_busy()
        && !state.get_is_albums_busy()
        && !state.get_is_artists_busy()
}

fn submit_and_wait(view: &SessionWindow) {
    search(view).invoke_submit(QUERY.into());
    assert!(view.pump(|| is_idle(view) && shown_counts(view) == [SEARCH_LIMIT; 3]));
}

fn first_song_id(view: &SessionWindow) -> String {
    search(view)
        .get_songs()
        .row_data(0)
        .map(|row| row.id.to_string())
        .unwrap_or_default()
}

#[test]
fn given_a_submitted_query_when_the_server_answers_then_every_section_shows_its_results() {
    let view = open();

    submit_and_wait(&view);

    let state = search(&view);
    assert!(state.get_has_more_songs());
    assert!(state.get_has_more_albums());
    assert!(state.get_has_more_artists());
}

#[test]
fn given_shown_results_when_more_is_asked_for_every_section_then_each_grows() {
    let view = open();
    submit_and_wait(&view);

    for kind in [SearchKind::Songs, SearchKind::Albums, SearchKind::Artists] {
        search(&view).invoke_show_more(kind);
    }

    assert!(view.pump(|| {
        is_idle(&view)
            && shown_counts(&view)
                .iter()
                .all(|count| *count > SEARCH_LIMIT)
    }));
}

#[test]
fn given_shown_cards_when_a_cover_is_needed_then_the_card_shows_it() {
    let view = open();
    submit_and_wait(&view);

    search(&view).invoke_need_cover(SearchKind::Albums, 0);
    search(&view).invoke_need_cover(SearchKind::Artists, -1);

    assert!(view.pump(|| {
        search(&view)
            .get_albums()
            .row_data(0)
            .is_some_and(|card| card.has_cover)
    }));
}

#[test]
fn given_found_songs_when_one_is_played_then_the_results_are_queued_from_it() {
    let view = open();
    submit_and_wait(&view);
    view.drain_commands();
    let song_id = first_song_id(&view);

    search(&view).invoke_play(0);

    match view.next_command() {
        Some(Command::PlaySongs { songs, song_id: id }) => {
            assert_eq!(songs.len(), SEARCH_LIMIT);
            assert_eq!(id, Some(song_id));
        }
        _ => panic!("expected PlaySongs"),
    }
}

#[test]
fn given_found_songs_when_one_is_played_alone_queued_next_or_last_then_only_it_is_sent() {
    let view = open();
    submit_and_wait(&view);
    view.drain_commands();
    let song_id = first_song_id(&view);
    let state = search(&view);

    state.invoke_play_only(0);
    state.invoke_play_next(0);
    state.invoke_add_to_queue(0);
    state.invoke_play(i32::MAX);

    let ids = |songs: &[cadence_core::model::Song]| -> Vec<String> {
        songs.iter().map(|song| song.id.clone()).collect()
    };
    assert!(matches!(
        view.next_command(),
        Some(Command::PlaySongs { songs, song_id: None }) if ids(&songs) == [song_id.clone()]
    ));
    assert!(matches!(
        view.next_command(),
        Some(Command::AddNext(songs)) if ids(&songs) == [song_id.clone()]
    ));
    assert!(matches!(
        view.next_command(),
        Some(Command::AddLast(songs)) if ids(&songs) == [song_id.clone()]
    ));
    assert!(view.next_command().is_none());
}

#[test]
fn given_an_edited_query_when_the_debounce_ends_then_it_searches() {
    let view = open();

    search(&view).invoke_edited(QUERY.into());

    assert!(view.pump(|| is_idle(&view) && shown_counts(&view) == [SEARCH_LIMIT; 3]));
}

#[test]
fn given_shown_results_when_the_query_is_erased_then_the_results_clear() {
    let view = open();
    submit_and_wait(&view);

    search(&view).invoke_edited("   ".into());

    assert_eq!(shown_counts(&view), [0; 3]);
    assert!(!search(&view).get_is_loading());
}

#[test]
fn given_a_query_from_another_page_when_search_loads_then_it_searches_for_it() {
    let view = open();

    search(&view).invoke_load(QUERY.into());

    assert_eq!(search(&view).get_query(), QUERY);
    assert!(view.pump(|| is_idle(&view) && shown_counts(&view) == [SEARCH_LIMIT; 3]));
    search(&view).invoke_load("".into());
    assert!(!search(&view).get_is_loading());
}

#[test]
fn given_an_offline_server_when_a_query_is_submitted_then_the_status_shows_the_failure() {
    let view = open();
    view.go_offline();

    search(&view).invoke_submit(QUERY.into());

    assert!(view.pump_until_status(&format!("Search failed: {OFFLINE}")));
    assert!(view.pump(|| is_idle(&view)));
    assert_eq!(shown_counts(&view), [0; 3]);
}

#[test]
fn given_shown_results_when_more_fails_then_the_status_shows_the_failure() {
    let view = open();
    submit_and_wait(&view);
    view.go_offline();

    search(&view).invoke_show_more(SearchKind::Songs);

    assert!(view.pump_until_status(&format!("Could not load more results: {OFFLINE}")));
    assert!(view.pump(|| is_idle(&view)));
    assert_eq!(shown_counts(&view), [SEARCH_LIMIT; 3]);
}

#[test]
fn given_no_server_when_searching_then_nothing_stays_loading() {
    let view = open();
    submit_and_wait(&view);
    view.disconnect();

    search(&view).invoke_show_more(SearchKind::Albums);
    search(&view).invoke_need_cover(SearchKind::Artists, 0);
    search(&view).invoke_submit("other".into());

    assert!(is_idle(&view));
    assert!(view.pump(|| true));
    assert!(view.status().is_empty());
}
