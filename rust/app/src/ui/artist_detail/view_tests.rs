use std::sync::Arc;

use cadence_api::MusicServer;
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, Model};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::list_stream::Placement;
use crate::tests::refusing_server::{REFUSAL, RefusingServer};
use crate::ui::test_window::TestWindow;
use crate::ui::{AppState, ArtistDetailState, PlayerState, Route};

fn first_demo_artist_id(server: &FakeServer) -> String {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let album = runtime.block_on(server.album("album-0")).unwrap();
    album.album.album_artists.first().unwrap().id.clone()
}

fn is_shown(view: &TestWindow, label: &str) -> bool {
    ElementHandle::find_by_accessible_label(&view.app, label)
        .next()
        .is_some()
}

#[test]
fn given_an_artist_with_top_songs_its_track_columns_are_labelled() {
    let server = Arc::new(FakeServer::new(200));
    let artist_id = first_demo_artist_id(&server);
    let Some(view) = TestWindow::open(server) else {
        return;
    };

    view.app
        .global::<AppState>()
        .invoke_navigate(Route::ArtistDetail, artist_id.into());

    assert!(view.pump(|| {
        view.app
            .global::<ArtistDetailState>()
            .get_top_songs()
            .row_count()
            > 0
    }));
    assert!(view.pump(|| {
        ["#", "Title", "Plays"]
            .iter()
            .all(|label| is_shown(&view, label))
    }));
}

fn open_artist_on(server: Arc<dyn MusicServer>, artist_id: &str) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::ArtistDetail, artist_id.into());
    let state = view.app.global::<ArtistDetailState>();
    assert!(view.pump(|| !state.get_is_loading()));
    drain_commands(&view);
    Some(view)
}

fn open_first_artist() -> Option<TestWindow> {
    let server = Arc::new(FakeServer::new(200));
    let artist_id = first_demo_artist_id(&server);
    open_artist_on(server, &artist_id)
}

fn drain_commands(view: &TestWindow) -> Vec<Command> {
    std::iter::from_fn(|| view.next_command()).collect()
}

fn wait_for_command(view: &TestWindow) -> Option<Command> {
    let command = std::cell::RefCell::new(None);
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
fn given_an_artist_when_opened_then_its_header_image_and_albums_are_shown() {
    let Some(view) = open_first_artist() else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();

    assert!(!state.get_name().is_empty());
    assert!(view.pump(|| state.get_has_image()));
    assert!(state.get_albums().row_count() > 0);

    state.invoke_album_shown(0);
    state.invoke_album_shown(-1);

    assert!(view.pump(|| {
        state
            .get_albums()
            .row_data(0)
            .is_some_and(|album| album.has_image)
    }));
}

#[test]
fn given_a_missing_artist_when_opened_then_the_page_says_so_and_cannot_be_played() {
    let Some(view) = open_artist_on(Arc::new(FakeServer::new(200)), "artist-missing") else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();

    assert_eq!(state.get_error(), "Could not load the artist: not found");

    state.invoke_play(false);
    state.invoke_toggle_favorite();
    assert!(drain_commands(&view).is_empty());
}

#[test]
fn given_an_artist_when_played_or_shuffled_then_their_songs_stream_from_the_library() {
    let Some(view) = open_first_artist() else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();

    state.invoke_play(false);
    state.invoke_play(true);
    view.app.global::<PlayerState>().set_is_shuffled(true);
    state.invoke_play(true);

    let commands = drain_commands(&view);
    assert!(matches!(
        commands.as_slice(),
        [
            Command::PlayFromList { song_id: None, .. },
            Command::ToggleShuffle,
            Command::EnqueueFromList {
                placement: Placement::Now,
                ..
            },
            Command::EnqueueFromList { .. },
        ]
    ));
}

#[test]
fn given_a_top_song_when_played_then_the_top_songs_start_from_it() {
    let Some(view) = open_first_artist() else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();
    let top_song = state.get_top_songs().row_data(0).unwrap().id;

    state.invoke_play_top(top_song.clone());

    let commands = drain_commands(&view);
    let [
        Command::PlaySongs {
            song_id: Some(played),
            songs,
        },
    ] = commands.as_slice()
    else {
        panic!("expected the top songs to play");
    };
    assert_eq!(played.as_str(), top_song.as_str());
    assert_eq!(songs.len(), state.get_top_songs().row_count());
}

#[test]
fn given_an_album_card_when_played_then_the_album_songs_are_sent_or_the_failure_shown() {
    let Some(view) = open_first_artist() else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();

    state.invoke_play_album("album-0".into());
    let played = wait_for_command(&view);
    assert!(
        matches!(played, Some(Command::PlaySongs { song_id: None, songs }) if !songs.is_empty())
    );

    state.invoke_play_album("album-missing".into());
    assert!(view.pump(|| view.status() == "Could not play the album: not found"));
}

#[test]
fn given_a_loaded_artist_when_favorited_then_the_heart_follows() {
    let Some(view) = open_first_artist() else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();
    let was_favorite = state.get_is_favorite();

    state.invoke_toggle_favorite();

    assert_eq!(state.get_is_favorite(), !was_favorite);
}

#[test]
fn given_a_server_refusing_changes_when_favoriting_then_the_heart_reverts_and_says_why() {
    let server = Arc::new(RefusingServer::new(200));
    let artist_id = first_demo_artist_id(&FakeServer::new(200));
    let Some(view) = open_artist_on(server, &artist_id) else {
        return;
    };
    let state = view.app.global::<ArtistDetailState>();
    let was_favorite = state.get_is_favorite();

    state.invoke_toggle_favorite();

    assert!(view.pump(|| view.status().starts_with("Could not update the favorite")));
    assert_eq!(
        view.status(),
        format!("Could not update the favorite: not authorized: {REFUSAL}")
    );
    assert_eq!(state.get_is_favorite(), was_favorite);
}
