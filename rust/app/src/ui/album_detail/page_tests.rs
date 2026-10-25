use std::sync::Arc;

use cadence_api::MusicServer;
use slint::{ComponentHandle, Model};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::tests::refusing_server::{REFUSAL, RefusingServer};
use crate::ui::test_window::TestWindow;
use crate::ui::{AlbumDetailState, AppState, PlayerState, Route};

const ALBUM: &str = "album-0";

fn open_album_on(server: Arc<dyn MusicServer>, album_id: &str) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::AlbumDetail, album_id.into());
    let state = view.app.global::<AlbumDetailState>();
    assert!(view.pump(|| !state.get_is_loading()));
    drain_commands(&view);
    Some(view)
}

fn open_album(album_id: &str) -> Option<TestWindow> {
    open_album_on(Arc::new(FakeServer::new(200)), album_id)
}

fn drain_commands(view: &TestWindow) -> Vec<Command> {
    std::iter::from_fn(|| view.next_command()).collect()
}

fn first_track_id(view: &TestWindow) -> String {
    let tracks = view.app.global::<AlbumDetailState>().get_tracks();
    (0..tracks.row_count())
        .filter_map(|row| tracks.row_data(row))
        .map(|track| track.id.to_string())
        .find(|id| !id.is_empty())
        .unwrap()
}

#[test]
fn given_a_demo_album_when_opened_then_its_header_tracks_and_cover_are_shown() {
    let Some(view) = open_album(ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();

    assert!(!state.get_name().is_empty());
    assert!(state.get_info().contains("songs"), "{}", state.get_info());
    assert!(state.get_artists().row_count() > 0);
    assert!(state.get_tracks().row_count() > 0);
    assert!(view.pump(|| state.get_has_cover()));
}

#[test]
fn given_a_missing_album_when_opened_then_the_page_and_status_say_so() {
    let Some(view) = open_album("album-missing") else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();

    assert_eq!(state.get_error(), "Album not found");
    assert_eq!(view.status(), "Could not load the album: not found");

    state.invoke_play();
    state.invoke_shuffle();
    state.invoke_play_next("song-1".into());
    assert!(drain_commands(&view).is_empty());
}

#[test]
fn given_an_open_album_when_played_or_shuffled_then_its_songs_are_sent_to_the_player() {
    let Some(view) = open_album(ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();
    let track_id = first_track_id(&view);

    state.invoke_play();
    state.invoke_shuffle();
    view.app.global::<PlayerState>().set_is_shuffled(true);
    state.invoke_shuffle();
    state.invoke_play_track(track_id.as_str().into());
    state.invoke_play_track("song-not-on-the-album".into());

    let commands = drain_commands(&view);
    let [
        Command::PlaySongs { song_id: None, .. },
        Command::ToggleShuffle,
        Command::PlaySongs { song_id: None, .. },
        Command::PlaySongs { song_id: None, .. },
        Command::PlaySongs {
            song_id: Some(played),
            ..
        },
    ] = commands.as_slice()
    else {
        panic!("expected play, shuffle twice and one track");
    };
    assert_eq!(played, &track_id);
}

#[test]
fn given_an_album_track_when_queued_then_it_goes_next_or_last() {
    let Some(view) = open_album(ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();
    let track_id = first_track_id(&view);

    state.invoke_play_next(track_id.as_str().into());
    state.invoke_add_to_queue(track_id.as_str().into());

    let commands = drain_commands(&view);
    let [Command::AddNext(next), Command::AddLast(last)] = commands.as_slice() else {
        panic!("expected one song next and one last");
    };
    assert_eq!(
        next.first().map(|song| song.id.as_str()),
        Some(track_id.as_str())
    );
    assert_eq!(last.len(), 1);
}

#[test]
fn given_an_album_when_favorited_and_rated_then_the_page_keeps_the_new_values() {
    let Some(view) = open_album(ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();
    let was_favorite = state.get_is_favorite();

    state.invoke_toggle_favorite();
    state.invoke_set_rating(4);

    assert_eq!(state.get_is_favorite(), !was_favorite);
    assert_eq!(state.get_rating(), 4);
}

#[test]
fn given_a_server_refusing_changes_when_favoriting_then_the_album_reverts_and_says_why() {
    let Some(view) = open_album_on(Arc::new(RefusingServer::new(200)), ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();
    let was_favorite = state.get_is_favorite();

    state.invoke_toggle_favorite();

    assert!(view.pump(|| state.get_is_favorite() == was_favorite));
    assert_eq!(
        view.status(),
        format!("Could not update favorite: not authorized: {REFUSAL}")
    );
}

#[test]
fn given_a_server_refusing_changes_when_rating_then_the_album_reverts_and_says_why() {
    let Some(view) = open_album_on(Arc::new(RefusingServer::new(200)), ALBUM) else {
        return;
    };
    let state = view.app.global::<AlbumDetailState>();
    let previous = state.get_rating();

    state.invoke_set_rating(previous + 1);

    assert!(view.pump(|| view.status().starts_with("Could not update rating")));
    assert_eq!(state.get_rating(), previous);
}
