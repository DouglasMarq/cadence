use std::sync::Arc;

use slint::{ComponentHandle, Model, SharedString};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::tests::offline_server::OFFLINE;
use crate::tests::session_window::SessionWindow;
use crate::ui::{AlbumGridCard, AlbumsState, AppState, PlaylistPickerState, Route};

const ALBUM_ID: &str = "album-1";
const SONGS_PER_ALBUM: usize = 12;

fn albums(view: &SessionWindow) -> AlbumsState<'_> {
    view.app.global::<AlbumsState>()
}

fn first_card(view: &SessionWindow) -> Option<AlbumGridCard> {
    albums(view).get_rows().row_data(0)?.cards.row_data(0)
}

fn open_albums() -> SessionWindow {
    let view = SessionWindow::open(Arc::new(FakeServer::new(2_000)));
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Albums, SharedString::default());
    assert!(view.pump(|| {
        !albums(&view).get_is_loading() && first_card(&view).is_some_and(|card| card.is_loaded)
    }));
    view
}

#[test]
fn given_the_albums_page_when_it_opens_then_the_count_cards_and_covers_load() {
    let view = open_albums();

    assert!(albums(&view).get_total() > 0);
    assert!(view.pump(|| first_card(&view).is_some_and(|card| card.has_cover)));
}

#[test]
fn given_an_album_when_it_is_played_queued_next_or_last_then_its_songs_are_sent() {
    let view = open_albums();
    view.drain_commands();
    let state = albums(&view);

    state.invoke_play(ALBUM_ID.into());
    state.invoke_play_next(ALBUM_ID.into());
    state.invoke_add_to_queue(ALBUM_ID.into());

    let commands = std::cell::RefCell::new(Vec::new());
    assert!(view.pump(|| {
        let mut received = commands.borrow_mut();
        received.extend(std::iter::from_fn(|| view.next_command()));
        received.len() == 3
    }));
    let commands = commands.into_inner();
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::PlaySongs { songs, song_id: None } if songs.len() == SONGS_PER_ALBUM
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::AddNext(songs) if songs.len() == SONGS_PER_ALBUM
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        Command::AddLast(songs) if songs.len() == SONGS_PER_ALBUM
    )));
}

#[test]
fn given_an_album_when_it_is_added_to_a_playlist_then_the_picker_opens_for_its_songs() {
    let view = open_albums();

    albums(&view).invoke_add_to_playlist(ALBUM_ID.into());

    let picker = view.app.global::<PlaylistPickerState>();
    assert!(view.pump(|| picker.get_is_open()));
    assert_eq!(
        usize::try_from(picker.get_song_count()).ok(),
        Some(SONGS_PER_ALBUM)
    );
}

#[test]
fn given_an_offline_server_when_an_album_is_played_then_the_status_shows_the_failure() {
    let view = open_albums();
    view.go_offline();

    albums(&view).invoke_play(ALBUM_ID.into());

    assert!(view.pump_until_status(&format!("Could not load the album: {OFFLINE}")));
}

#[test]
fn given_an_offline_server_when_the_albums_reload_then_the_count_fails_with_an_error() {
    let view = open_albums();
    view.go_offline();

    albums(&view).invoke_query_changed();

    assert!(view.pump_until_status(&format!("Could not load albums: {OFFLINE}")));
    assert!(view.pump(|| !albums(&view).get_is_loading()));
    assert_eq!(albums(&view).get_total(), 0);
}

#[test]
fn given_shown_albums_when_a_later_page_fails_then_the_status_shows_the_failure() {
    let view = open_albums();
    let total = usize::try_from(albums(&view).get_total()).unwrap_or_default();
    view.go_offline();

    let rows = albums(&view).get_rows();
    rows.row_data(rows.row_count().saturating_sub(1));

    assert!(total > 0);
    assert!(view.pump_until_status(&format!("Could not load albums: {OFFLINE}")));
}

#[test]
fn given_no_server_when_an_album_is_played_then_nothing_is_sent() {
    let view = open_albums();
    view.drain_commands();
    view.disconnect();

    albums(&view).invoke_play(ALBUM_ID.into());
    albums(&view).invoke_query_changed();

    assert!(view.pump(|| true));
    assert!(view.next_command().is_none());
}
