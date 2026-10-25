use std::cell::RefCell;
use std::sync::Arc;

use cadence_api::MusicServer;
use slint::{ComponentHandle, Model};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::tests::refusing_server::{REFUSAL, RefusingServer};
use crate::ui::test_window::TestWindow;
use crate::ui::{AppState, PlaylistPlayMode, PlaylistsState, Route};

const WORKOUT: &str = "playlist-2";

fn open_playlists_on(server: Arc<dyn MusicServer>) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Playlists, "".into());
    assert!(view.pump(|| row_count(&view) > 0));
    Some(view)
}

fn open_playlists() -> Option<TestWindow> {
    open_playlists_on(Arc::new(FakeServer::new(200)))
}

fn row_count(view: &TestWindow) -> usize {
    view.app.global::<PlaylistsState>().get_rows().row_count()
}

fn next_command_after(view: &TestWindow, play: impl Fn()) -> Option<Command> {
    std::iter::from_fn(|| view.next_command()).for_each(drop);
    play();
    let command = RefCell::new(None);
    view.pump(|| {
        let mut received = command.borrow_mut();
        if received.is_none() {
            *received = view.next_command();
        }
        received.is_some()
    });
    command.into_inner()
}

#[test]
fn given_the_playlists_page_when_opened_then_it_lists_the_playlists_with_their_covers() {
    let Some(view) = open_playlists() else {
        return;
    };
    let rows = view.app.global::<PlaylistsState>().get_rows();

    assert_eq!(rows.row_data(1).unwrap().name, "Workout");
    assert!(view.pump(|| {
        (0..rows.row_count()).any(|row| rows.row_data(row).is_some_and(|row| row.has_cover))
    }));
    assert!(!view.app.global::<PlaylistsState>().get_is_loading());
}

#[test]
fn given_a_playlist_when_played_in_each_mode_then_its_songs_are_sent_to_the_player() {
    let Some(view) = open_playlists() else {
        return;
    };
    let state = view.app.global::<PlaylistsState>();

    let now = next_command_after(&view, || {
        state.invoke_play(WORKOUT.into(), PlaylistPlayMode::Now)
    });
    let next = next_command_after(&view, || {
        state.invoke_play(WORKOUT.into(), PlaylistPlayMode::Next)
    });
    let last = next_command_after(&view, || {
        state.invoke_play(WORKOUT.into(), PlaylistPlayMode::Last)
    });

    assert!(matches!(now, Some(Command::PlaySongs { song_id: None, songs }) if !songs.is_empty()));
    assert!(matches!(next, Some(Command::AddNext(songs)) if !songs.is_empty()));
    assert!(matches!(last, Some(Command::AddLast(songs)) if !songs.is_empty()));
}

#[test]
fn given_an_empty_or_missing_playlist_when_played_then_the_status_says_why() {
    let server = Arc::new(FakeServer::new(200));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let empty = runtime
        .block_on(server.create_playlist("Empty", &[]))
        .unwrap();
    let Some(view) = open_playlists_on(server) else {
        return;
    };
    let state = view.app.global::<PlaylistsState>();

    state.invoke_play(empty.id.into(), PlaylistPlayMode::Now);
    assert!(view.pump(|| view.status() == "This playlist is empty"));

    state.invoke_play("playlist-missing".into(), PlaylistPlayMode::Now);
    assert!(view.pump(|| view.status() == "Could not load the playlist: not found"));
}

#[test]
fn given_a_name_when_creating_a_playlist_then_it_appears_in_the_list() {
    let Some(view) = open_playlists() else {
        return;
    };
    let state = view.app.global::<PlaylistsState>();
    let before = row_count(&view);
    state.set_is_creating(true);

    state.invoke_create("   ".into());
    assert!(!state.get_is_busy());
    state.invoke_create(" Road trip ".into());

    assert!(state.get_is_busy());
    assert!(view.pump(|| row_count(&view) == before + 1));
    assert_eq!(view.status(), "Created playlist \"Road trip\"");
    assert!(!state.get_is_creating());
}

#[test]
fn given_a_playlist_when_deleted_then_it_leaves_the_list() {
    let Some(view) = open_playlists() else {
        return;
    };
    let state = view.app.global::<PlaylistsState>();
    let before = row_count(&view);
    state.set_delete_id(WORKOUT.into());
    state.set_delete_name("Workout".into());

    state.invoke_delete(WORKOUT.into());

    assert!(view.pump(|| row_count(&view) == before - 1));
    assert_eq!(view.status(), "Deleted playlist \"Workout\"");
    assert_eq!(state.get_delete_id(), "");

    state.invoke_delete(WORKOUT.into());
    assert!(view.pump(|| view.status() == "Could not delete the playlist: not found"));
}

#[test]
fn given_a_server_refusing_changes_when_creating_a_playlist_then_the_status_says_why() {
    let Some(view) = open_playlists_on(Arc::new(RefusingServer::new(200))) else {
        return;
    };
    let state = view.app.global::<PlaylistsState>();

    state.invoke_create("Road trip".into());

    assert!(view.pump(|| !state.get_is_busy()));
    assert_eq!(
        view.status(),
        format!("Could not create the playlist: not authorized: {REFUSAL}")
    );
}
