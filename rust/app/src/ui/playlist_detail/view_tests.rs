use std::sync::Arc;

use cadence_api::MusicServer;
use slint::{ComponentHandle, Model};

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::list_stream::{ListRow, Placement, PlaylistId, SongSource};
use crate::tests::refusing_server::{REFUSAL, RefusingServer};
use crate::ui::test_window::TestWindow;
use crate::ui::{
    AppState, PlayerState, PlaylistDetailState, PlaylistPickerState, PlaylistSongAction, Route,
};

const FIRST_PLAYLIST: &str = "playlist-1";
const LONG_PLAYLIST_SONGS: usize = 1_200;

fn open_with(server: Arc<dyn MusicServer>, playlist_id: &str) -> Option<TestWindow> {
    let view = TestWindow::open(server)?;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::PlaylistDetail, playlist_id.into());
    let state = view.app.global::<PlaylistDetailState>();
    assert!(view.pump(|| !state.get_is_loading()));
    drain_commands(&view);
    Some(view)
}

fn open_playlist(playlist_id: &str) -> Option<TestWindow> {
    open_with(Arc::new(FakeServer::new(200)), playlist_id)
}

fn drain_commands(view: &TestWindow) -> Vec<Command> {
    std::iter::from_fn(|| view.next_command()).collect()
}

fn row_count(view: &TestWindow) -> usize {
    view.app
        .global::<PlaylistDetailState>()
        .get_rows()
        .row_count()
}

fn long_playlist_server() -> (Arc<FakeServer>, String) {
    let server = Arc::new(FakeServer::new(2_000));
    let song_ids: Vec<String> = (0..LONG_PLAYLIST_SONGS)
        .map(|index| format!("song-{index}"))
        .collect();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let created = runtime
        .block_on(server.create_playlist("Long", &song_ids))
        .unwrap();
    (server, created.id)
}

fn is_playlist_source(source: &SongSource, playlist_id: &str) -> bool {
    matches!(source, SongSource::Playlist(PlaylistId(id)) if id == playlist_id)
}

#[test]
fn given_a_demo_playlist_when_opened_then_its_header_rows_and_cover_are_shown() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();

    assert_eq!(state.get_name(), "Morning Coffee");
    assert!(state.get_comment().starts_with("A demo playlist with"));
    assert!(state.get_info().contains("songs"), "{}", state.get_info());
    assert!(row_count(&view) > 0);
    assert!(
        state
            .get_rows()
            .row_data(0)
            .is_some_and(|row| row.track.is_loaded)
    );
    assert!(view.pump(|| state.get_has_cover()));
}

#[test]
fn given_a_missing_playlist_when_opened_then_the_status_reports_the_failure() {
    let Some(view) = open_playlist("playlist-missing") else {
        return;
    };

    assert_eq!(view.status(), "Could not load the playlist: not found");
    assert_eq!(row_count(&view), 0);
}

#[test]
fn given_no_playlist_id_when_loading_then_nothing_is_fetched() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();

    state.invoke_load("".into());
    state.invoke_play(false);

    assert!(!state.get_is_loading());
    assert_eq!(row_count(&view), 0);
    assert!(drain_commands(&view).is_empty());
}

#[test]
fn given_an_open_playlist_when_played_then_it_streams_from_the_top_or_from_a_row() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();

    state.invoke_play(false);
    state.invoke_play_row(2);
    state.invoke_play_row(100_000);

    let commands = drain_commands(&view);
    let [
        Command::EnqueueFromList {
            source: from_top,
            placement: Placement::Now,
            ..
        },
        Command::PlayFromList {
            source: from_row,
            song_id: Some(_),
            row_index: ListRow(2),
        },
    ] = commands.as_slice()
    else {
        panic!("expected a play from the top and a play from row 2");
    };
    assert!(is_playlist_source(from_top, FIRST_PLAYLIST));
    assert!(is_playlist_source(from_row, FIRST_PLAYLIST));
}

#[test]
fn given_shuffle_is_off_when_shuffling_the_playlist_then_shuffle_turns_on_first() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };

    view.app.global::<PlaylistDetailState>().invoke_play(true);
    view.app.global::<PlayerState>().set_is_shuffled(true);
    view.app.global::<PlaylistDetailState>().invoke_play(true);

    let commands = drain_commands(&view);
    assert!(matches!(
        commands.as_slice(),
        [
            Command::ToggleShuffle,
            Command::EnqueueFromList { .. },
            Command::EnqueueFromList { .. },
        ]
    ));
}

#[test]
fn given_rows_when_selected_then_the_selection_count_follows() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();

    state.invoke_select(1, false, false);
    state.invoke_select(3, false, true);
    assert_eq!(state.get_selected_count(), 3);
    state.invoke_select(-1, false, false);
    assert_eq!(state.get_selected_count(), 3);

    state.invoke_select_all();
    assert_eq!(
        usize::try_from(state.get_selected_count()).unwrap(),
        row_count(&view)
    );

    state.invoke_clear_selection();
    assert_eq!(state.get_selected_count(), 0);
}

#[test]
fn given_a_row_menu_when_queueing_then_the_rows_are_enqueued_where_asked() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();

    state.invoke_context_action(0, PlaylistSongAction::Play);
    state.invoke_context_action(1, PlaylistSongAction::PlayNext);
    state.invoke_context_action(2, PlaylistSongAction::AddToQueue);
    state.invoke_context_action(100_000, PlaylistSongAction::Play);

    let placements: Vec<Placement> = drain_commands(&view)
        .into_iter()
        .filter_map(|command| match command {
            Command::EnqueueFromList {
                source, placement, ..
            } if is_playlist_source(&source, FIRST_PLAYLIST) => Some(placement),
            _ => None,
        })
        .collect();
    assert_eq!(
        placements,
        [Placement::Now, Placement::Next, Placement::Last]
    );
}

#[test]
fn given_a_row_when_removed_from_the_menu_then_the_playlist_reloads_without_it() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let before = row_count(&view);

    view.app
        .global::<PlaylistDetailState>()
        .invoke_context_action(0, PlaylistSongAction::Remove);

    assert!(view.pump(|| row_count(&view) == before - 1));
    assert_eq!(view.status(), "Removed 1 song from \"Morning Coffee\"");
}

#[test]
fn given_selected_rows_when_removed_then_the_status_counts_them() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();
    let before = row_count(&view);
    state.invoke_select(0, false, false);
    state.invoke_select(1, false, true);

    state.invoke_remove_selected();

    assert!(state.get_is_busy());
    assert!(view.pump(|| row_count(&view) == before - 2));
    assert!(!state.get_is_busy());
    assert_eq!(view.status(), "Removed 2 songs from \"Morning Coffee\"");
}

#[test]
fn given_a_server_refusing_changes_when_removing_then_the_status_shows_the_refusal() {
    let Some(view) = open_with(Arc::new(RefusingServer::new(200)), FIRST_PLAYLIST) else {
        return;
    };
    let before = row_count(&view);

    view.app
        .global::<PlaylistDetailState>()
        .invoke_context_action(0, PlaylistSongAction::Remove);

    assert!(view.pump(|| view.status().starts_with("Could not remove songs")));
    assert_eq!(
        view.status(),
        format!("Could not remove songs: not authorized: {REFUSAL}")
    );
    assert_eq!(row_count(&view), before);
}

#[test]
fn given_loaded_selected_rows_when_added_to_a_playlist_then_the_picker_opens_for_them() {
    let Some(view) = open_playlist(FIRST_PLAYLIST) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();
    let picker = view.app.global::<PlaylistPickerState>();

    state.invoke_add_selected_to_playlist();
    assert!(!picker.get_is_open());

    state.invoke_select(0, false, false);
    state.invoke_select(2, false, true);
    state.invoke_add_selected_to_playlist();

    assert!(picker.get_is_open());
    assert_eq!(picker.get_song_count(), 3);
    assert!(view.pump(|| picker.get_rows().row_count() > 0));
}

#[test]
fn given_a_long_playlist_when_scrolled_far_then_the_later_page_is_fetched() {
    let (server, playlist_id) = long_playlist_server();
    let Some(view) = open_with(server, &playlist_id) else {
        return;
    };
    let rows = view.app.global::<PlaylistDetailState>().get_rows();
    assert_eq!(rows.row_count(), LONG_PLAYLIST_SONGS);

    assert!(rows.row_data(1_000).is_some_and(|row| !row.track.is_loaded));

    assert!(view.pump(|| rows.row_data(1_000).is_some_and(|row| row.track.is_loaded)));
}

#[test]
fn given_a_long_playlist_that_grew_on_the_server_when_a_later_page_arrives_then_the_selection_is_dropped()
 {
    let (server, playlist_id) = long_playlist_server();
    let Some(view) = open_with(server.clone(), &playlist_id) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();
    state.invoke_select(0, false, false);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(server.add_to_playlist(&playlist_id, &["song-1500".into()]))
        .unwrap();

    state.get_rows().row_data(1_000);

    assert!(view.pump(|| row_count(&view) == LONG_PLAYLIST_SONGS + 1));
    assert_eq!(state.get_selected_count(), 0);
}

#[test]
fn given_a_long_playlist_deleted_on_the_server_when_a_later_page_fails_then_the_status_says_so() {
    let (server, playlist_id) = long_playlist_server();
    let Some(view) = open_with(server.clone(), &playlist_id) else {
        return;
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(server.delete_playlist(&playlist_id))
        .unwrap();

    view.app
        .global::<PlaylistDetailState>()
        .get_rows()
        .row_data(1_000);

    assert!(view.pump(|| view.status() == "Could not load the playlist: not found"));
}

#[test]
fn given_a_whole_long_playlist_selected_when_added_to_another_then_its_songs_stream_in() {
    let (server, playlist_id) = long_playlist_server();
    let Some(view) = open_with(server.clone(), &playlist_id) else {
        return;
    };
    let state = view.app.global::<PlaylistDetailState>();
    let picker = view.app.global::<PlaylistPickerState>();
    state.invoke_select_all();

    state.invoke_context_action(5, PlaylistSongAction::AddToPlaylist);
    assert!(picker.get_is_open());
    assert_eq!(
        usize::try_from(picker.get_song_count()).unwrap(),
        LONG_PLAYLIST_SONGS
    );
    assert!(view.pump(|| !picker.get_is_loading()));
    picker.invoke_pick("playlist-2".into());

    assert!(!picker.get_is_open());
    assert!(view.pump(|| view.status().starts_with("Added")));
    assert!(
        view.status().ends_with("songs to \"Workout\""),
        "{}",
        view.status()
    );
}
