use std::sync::Arc;

use cadence_api::MusicServer;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::demo::FakeServer;
use crate::tests::refusing_server::{REFUSAL, RefusingServer};
use crate::ui::test_window::TestWindow;
use crate::ui::{
    Actions, AppState, PlaylistDetailState, PlaylistPickerState, PlaylistSongAction, Route,
};

const WORKOUT: &str = "playlist-2";
const LONG_PLAYLIST_SONGS: usize = 1_200;

fn block_on<Output>(future: impl Future<Output = Output>) -> Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn song_ids_of(server: &dyn MusicServer, playlist_id: &str) -> Vec<String> {
    let songs = block_on(server.playlist_songs(playlist_id)).unwrap();
    songs.into_iter().map(|song| song.id).collect()
}

fn songs_missing_from(server: &dyn MusicServer, playlist_id: &str) -> [String; 2] {
    let existing = song_ids_of(server, playlist_id);
    let mut missing = (0..)
        .map(|index| format!("song-{index}"))
        .filter(|song_id| !existing.contains(song_id));
    [missing.next().unwrap(), missing.next().unwrap()]
}

fn open_picker(view: &TestWindow, song_ids: &[&str]) {
    let song_ids: Vec<SharedString> = song_ids.iter().map(|song_id| (*song_id).into()).collect();
    view.app
        .global::<Actions>()
        .invoke_add_to_playlist(ModelRc::new(VecModel::from(song_ids)));
    let picker = view.app.global::<PlaylistPickerState>();
    assert!(view.pump(|| !picker.get_is_loading()));
}

fn picked_view(server: Arc<dyn MusicServer>, song_ids: &[&str]) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    open_picker(&view, song_ids);
    Some(view)
}

fn wait_for_status(view: &TestWindow, prefix: &str) -> String {
    assert!(
        view.pump(|| view.status().starts_with(prefix)),
        "{}",
        view.status()
    );
    view.status()
}

#[test]
fn given_songs_to_add_when_the_picker_opens_then_it_lists_the_playlists_and_filters_them() {
    let Some(view) = picked_view(Arc::new(FakeServer::new(200)), &["song-1", "song-2"]) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();

    assert!(picker.get_is_open());
    assert_eq!(picker.get_song_count(), 2);
    assert!(picker.get_rows().row_count() > 1);

    picker.set_filter(" WORK ".into());
    picker.invoke_filter_edited(" WORK ".into());

    let rows = picker.get_rows();
    assert_eq!(rows.row_count(), 1);
    assert_eq!(rows.row_data(0).unwrap().name, "Workout");
}

#[test]
fn given_no_songs_when_adding_to_a_playlist_then_the_picker_stays_closed() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(200))) else {
        return;
    };

    open_picker(&view, &[]);

    assert!(!view.app.global::<PlaylistPickerState>().get_is_open());
}

#[test]
fn given_the_open_picker_when_cancelled_then_it_closes_and_ignores_the_late_playlists() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(200))) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();
    view.app
        .global::<Actions>()
        .invoke_add_to_playlist(ModelRc::new(VecModel::from(vec![SharedString::from(
            "song-1",
        )])));

    picker.invoke_cancel();

    assert!(!picker.get_is_open());
    assert!(view.pump(|| !picker.get_is_loading() || picker.get_rows().row_count() == 0));
    picker.invoke_pick(WORKOUT.into());
    assert!(!picker.get_is_busy());
}

#[test]
fn given_new_songs_when_picking_a_playlist_then_they_are_added_and_the_picker_closes() {
    let server = Arc::new(FakeServer::new(200));
    let [first, second] = songs_missing_from(server.as_ref(), WORKOUT);
    let Some(view) = picked_view(server.clone(), &[&first, &second, &first]) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();
    let app = view.app.global::<AppState>();
    app.invoke_navigate(Route::PlaylistDetail, WORKOUT.into());

    picker.invoke_pick(WORKOUT.into());

    assert!(picker.get_is_busy());
    let status = wait_for_status(&view, "Added");
    assert_eq!(status, "Added 2 songs to \"Workout\"");
    assert!(!picker.get_is_open());
    assert!(song_ids_of(server.as_ref(), WORKOUT).contains(&second));
}

#[test]
fn given_songs_already_in_the_playlist_when_picking_it_then_nothing_is_added() {
    let server = Arc::new(FakeServer::new(200));
    let existing = song_ids_of(server.as_ref(), WORKOUT);
    let Some(view) = picked_view(server, &[existing.first().unwrap()]) else {
        return;
    };

    view.app
        .global::<PlaylistPickerState>()
        .invoke_pick(WORKOUT.into());

    assert_eq!(wait_for_status(&view, "Already"), "Already in \"Workout\"");
}

#[test]
fn given_a_name_when_creating_a_playlist_then_it_is_created_with_the_songs() {
    let Some(view) = picked_view(Arc::new(FakeServer::new(200)), &["song-1", "song-2"]) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Playlists, "".into());

    picker.invoke_create("   ".into());
    assert!(!picker.get_is_busy());
    picker.invoke_create("  Road trip ".into());

    assert_eq!(
        wait_for_status(&view, "Created"),
        "Created \"Road trip\" with 2 songs"
    );
    assert!(!picker.get_is_open());
}

#[test]
fn given_a_server_refusing_changes_when_picking_or_creating_then_the_picker_shows_why() {
    let server = Arc::new(RefusingServer::new(200));
    let [missing, _] = songs_missing_from(server.as_ref(), WORKOUT);
    let Some(view) = picked_view(server, &[&missing]) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();

    picker.invoke_pick(WORKOUT.into());
    assert_eq!(
        wait_for_status(&view, "Could not add"),
        format!("Could not add to \"Workout\": not authorized: {REFUSAL}")
    );
    assert!(picker.get_is_open());
    assert!(!picker.get_is_busy());

    picker.invoke_create("Road trip".into());
    assert_eq!(
        wait_for_status(&view, "Could not create"),
        format!("Could not create the playlist: not authorized: {REFUSAL}")
    );
    assert!(picker.get_is_open());
}

fn long_playlist(server: &FakeServer) -> String {
    let song_ids: Vec<String> = (0..LONG_PLAYLIST_SONGS)
        .map(|index| format!("song-{index}"))
        .collect();
    block_on(server.create_playlist("Long", &song_ids))
        .unwrap()
        .id
}

fn pick_whole_playlist(server: Arc<dyn MusicServer>, playlist_id: &str) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::PlaylistDetail, playlist_id.into());
    let detail = view.app.global::<PlaylistDetailState>();
    assert!(view.pump(|| detail.get_rows().row_count() > 0));
    detail.invoke_select_all();
    detail.invoke_context_action(0, PlaylistSongAction::AddToPlaylist);
    let picker = view.app.global::<PlaylistPickerState>();
    assert!(view.pump(|| !picker.get_is_loading()));
    Some(view)
}

#[test]
fn given_a_whole_long_playlist_when_creating_a_playlist_from_it_then_every_song_is_copied() {
    let server = Arc::new(FakeServer::new(2_000));
    let playlist_id = long_playlist(&server);
    let Some(view) = pick_whole_playlist(server, &playlist_id) else {
        return;
    };

    view.app
        .global::<PlaylistPickerState>()
        .invoke_create("Copy".into());

    assert_eq!(
        wait_for_status(&view, "Created"),
        "Created \"Copy\" with 1200 songs"
    );
}

#[test]
fn given_a_server_refusing_changes_when_copying_a_long_selection_then_the_status_shows_why() {
    let server = Arc::new(RefusingServer::new(2_000));
    let Some(view) = pick_whole_playlist(server, WORKOUT) else {
        return;
    };
    let picker = view.app.global::<PlaylistPickerState>();

    picker.invoke_create("Copy".into());

    assert_eq!(
        wait_for_status(&view, "Could not create"),
        format!("Could not create the playlist: not authorized: {REFUSAL}")
    );
}
