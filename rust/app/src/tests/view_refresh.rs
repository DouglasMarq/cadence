use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cadence_api::MusicServer;
use cadence_core::model::{
    AlbumDetail, ArtistDetail, Paging, Playlist, ServerConfig, Song, SongListQuery,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::held_server::HeldServer;
use super::offline_server::OfflineServer;
use super::restart_views::{
    Destination, SONGS, block_on, created_playlist_id, first_artist_id, library_of,
    shows_album_detail, shows_artist_detail, shows_artists, shows_home_albums,
    shows_playlist_detail, shows_stations, shows_tracks, visited,
};
use super::session_window::SessionWindow;
use crate::demo::{DemoLibrary, FakeServer};
use crate::ui::{
    Actions, AlbumDetailState, AppState, ArtistDetailState, HomeState, PlaylistPickerState,
    PlaylistsState, RadioState, Route,
};
use crate::views::{View, ViewSlot};

const FEW_SONGS: usize = 40;
const NOT_YET_SHOWN_WATCH: Duration = Duration::from_millis(300);
const DEMO_SERVER: &str = "demo";
const DETAIL_ITEM: &str = "detail";
const STALE_NAME: &str = "Stale cached name";

type FirstPage = (Option<usize>, Vec<Song>);

fn album_page() -> Destination {
    Destination {
        route: Route::AlbumDetail,
        detail_id: "album-0".into(),
        is_shown: shows_album_detail,
    }
}

fn artist_page(library: &FakeServer) -> Destination {
    Destination {
        route: Route::ArtistDetail,
        detail_id: first_artist_id(library),
        is_shown: shows_artist_detail,
    }
}

fn song_outside_the_playlist(library: &FakeServer) -> SharedString {
    let paging = Paging {
        start_index: SONGS / 2,
        limit: 1,
    };
    let songs = block_on(library.song_list(&SongListQuery::default(), paging)).unwrap();
    songs.items.first().unwrap().id.as_str().into()
}

fn held_on(library: Arc<FakeServer>) -> Arc<HeldServer> {
    Arc::new(HeldServer::new(library))
}

fn served_by(held: &Arc<HeldServer>) -> Arc<dyn MusicServer> {
    Arc::clone(held) as Arc<dyn MusicServer>
}

fn another_server() -> Arc<HeldServer> {
    let config = ServerConfig {
        id: "another-server".into(),
        ..FakeServer::config_for_demo()
    };
    held_on(Arc::new(FakeServer::with_config(
        config,
        DemoLibrary::with_songs(SONGS),
    )))
}

fn stays_hidden(run: &SessionWindow, destination: &Destination) -> bool {
    let started = Instant::now();
    run.pump(|| destination.is_shown_in(run) || started.elapsed() > NOT_YET_SHOWN_WATCH);
    !destination.is_shown_in(run)
}

fn seed<Payload: crate::views::Snapshot>(run: &SessionWindow, slot: &ViewSlot, payload: Payload) {
    let store = run.context.store.views();
    block_on(store.seed(DEMO_SERVER, slot, payload));
}

fn corrupt_every_cached_entry(run: &SessionWindow) {
    let entries: Vec<_> = fs::read_dir(run.cache_directory())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(!entries.is_empty(), "the first run cached something");
    for entry in entries {
        fs::write(entry, b"{ not a cached view").unwrap();
    }
}

fn assert_failed_refresh_keeps_the_view(library: Arc<FakeServer>, destination: &Destination) {
    let first_run = visited(library, destination);
    let second_run = first_run.restart(Arc::new(OfflineServer::default()));

    destination.open_in(&second_run);

    assert!(second_run.pump(|| destination.is_shown_in(&second_run)));
    assert!(second_run.wait_until_idle(), "the failed refresh ended");
    assert!(destination.is_shown_in(&second_run));
    assert_eq!(second_run.status(), "");
}

fn assert_another_server_sees_nothing_cached(library: Arc<FakeServer>, destination: &Destination) {
    let first_run = visited(library, destination);
    let another = another_server();
    let second_run = first_run.restart(served_by(&another));

    destination.open_in(&second_run);

    assert!(stays_hidden(&second_run, destination));
    another.release();
    assert!(second_run.pump(|| destination.is_shown_in(&second_run)));
}

fn assert_corrupt_cache_is_ignored(library: Arc<FakeServer>, destination: &Destination) {
    let first_run = visited(Arc::clone(&library), destination);
    corrupt_every_cached_entry(&first_run);
    let held = held_on(library);
    let second_run = first_run.restart(served_by(&held));

    destination.open_in(&second_run);

    assert!(stays_hidden(&second_run, destination));
    held.release();
    assert!(second_run.pump(|| destination.is_shown_in(&second_run)));
    assert_eq!(second_run.status(), "");
}

#[test]
fn given_cached_artists_when_the_refresh_fails_then_they_stay_without_an_error() {
    let destination = Destination::page(Route::Artists, shows_artists);
    assert_failed_refresh_keeps_the_view(library_of(SONGS), &destination);
}

#[test]
fn given_artists_cached_for_a_server_when_another_server_opens_them_then_it_sees_nothing_cached() {
    let destination = Destination::page(Route::Artists, shows_artists);
    assert_another_server_sees_nothing_cached(library_of(SONGS), &destination);
}

#[test]
fn given_a_corrupt_artist_cache_when_restarting_then_the_server_answer_is_shown_silently() {
    let destination = Destination::page(Route::Artists, shows_artists);
    assert_corrupt_cache_is_ignored(library_of(SONGS), &destination);
}

#[test]
fn given_cached_tracks_when_the_refresh_fails_then_they_stay_without_an_error() {
    let destination = Destination::page(Route::Tracks, shows_tracks);
    assert_failed_refresh_keeps_the_view(library_of(SONGS), &destination);
}

#[test]
fn given_a_stale_cached_album_when_the_refresh_answers_then_the_page_shows_the_new_album() {
    let library = library_of(SONGS);
    let held = held_on(Arc::clone(&library));
    let run = SessionWindow::open(served_by(&held));
    let mut stale: AlbumDetail = block_on(library.album("album-0")).unwrap();
    let fresh_name = stale.album.name.clone();
    stale.album.name = STALE_NAME.into();
    let slot = View::Album("album-0".into()).slot(DETAIL_ITEM);
    seed(&run, &slot, stale);
    let state = run.app.global::<AlbumDetailState>();
    album_page().open_in(&run);
    assert!(run.pump(|| state.get_name() == STALE_NAME));
    let tracks = state.get_tracks().row_count();

    held.release();

    assert!(run.pump(|| state.get_name() == fresh_name.as_str()));
    assert_eq!(state.get_tracks().row_count(), tracks);
}

#[test]
fn given_a_stale_cached_artist_when_the_refresh_answers_then_the_page_shows_the_new_artist() {
    let library = library_of(SONGS);
    let held = held_on(Arc::clone(&library));
    let run = SessionWindow::open(served_by(&held));
    let destination = artist_page(&library);
    let mut stale: ArtistDetail = block_on(library.artist(&destination.detail_id)).unwrap();
    let fresh_name = stale.artist.name.clone();
    stale.artist.name = STALE_NAME.into();
    let slot = View::Artist(destination.detail_id.clone()).slot(DETAIL_ITEM);
    seed(&run, &slot, stale);
    let state = run.app.global::<ArtistDetailState>();
    destination.open_in(&run);
    assert!(run.pump(|| state.get_name() == STALE_NAME));

    held.release();

    assert!(run.pump(|| state.get_name() == fresh_name.as_str()));
    assert!(destination.is_shown_in(&run));
}

#[test]
fn given_a_cached_album_when_the_refresh_fails_then_it_stays_without_an_error() {
    assert_failed_refresh_keeps_the_view(library_of(SONGS), &album_page());
}

#[test]
fn given_a_cached_artist_when_the_refresh_fails_then_it_stays_without_an_error() {
    let library = library_of(SONGS);
    let destination = artist_page(&library);
    assert_failed_refresh_keeps_the_view(library, &destination);
}

#[test]
fn given_a_cached_playlist_when_the_refresh_fails_then_it_stays_without_an_error() {
    let library = library_of(SONGS);
    let destination = Destination {
        route: Route::PlaylistDetail,
        detail_id: created_playlist_id(&library),
        is_shown: shows_playlist_detail,
    };
    assert_failed_refresh_keeps_the_view(library, &destination);
}

#[test]
fn given_cached_radio_stations_when_the_refresh_fails_then_they_stay_without_an_error() {
    let destination = Destination::page(Route::Radio, shows_stations);
    assert_failed_refresh_keeps_the_view(library_of(SONGS), &destination);
}

#[test]
fn given_an_album_cached_for_a_server_when_another_server_opens_it_then_it_sees_nothing_cached() {
    assert_another_server_sees_nothing_cached(library_of(SONGS), &album_page());
}

#[test]
fn given_a_corrupt_album_cache_when_restarting_then_the_server_answer_is_shown_silently() {
    assert_corrupt_cache_is_ignored(library_of(SONGS), &album_page());
}

#[test]
fn given_cached_shelves_when_the_refresh_answers_then_their_albums_are_replaced() {
    let destination = Destination::page(Route::Home, shows_home_albums);
    let first_run = visited(library_of(FEW_SONGS), &destination);
    let held = held_on(library_of(SONGS));
    let run = first_run.restart(served_by(&held));
    destination.open_in(&run);
    assert!(run.pump(|| destination.is_shown_in(&run)));
    let shelf_albums = |shelf: usize| {
        let shelves = run.app.global::<HomeState>().get_shelves();
        shelves
            .row_data(shelf)
            .map(|shelf| shelf.albums.row_count())
    };
    let cached_albums = shelf_albums(0);

    held.release();

    assert!(run.pump(|| shelf_albums(0) > cached_albums));
}

#[test]
fn given_cached_shelves_when_the_refresh_fails_then_they_stay() {
    let destination = Destination::page(Route::Home, shows_home_albums);
    let first_run = visited(library_of(SONGS), &destination);
    let run = first_run.restart(Arc::new(OfflineServer::default()));

    destination.open_in(&run);

    assert!(run.pump(|| destination.is_shown_in(&run)));
    assert!(run.wait_until_idle(), "the failed refresh ended");
    assert!(destination.is_shown_in(&run));
}

#[test]
fn given_shelves_cached_for_a_server_when_another_server_opens_home_then_it_sees_nothing_cached() {
    let destination = Destination::page(Route::Home, shows_home_albums);
    assert_another_server_sees_nothing_cached(library_of(SONGS), &destination);
}

#[test]
fn given_a_corrupt_shelf_cache_when_restarting_then_the_server_answer_is_shown() {
    let destination = Destination::page(Route::Home, shows_home_albums);
    assert_corrupt_cache_is_ignored(library_of(SONGS), &destination);
}

#[test]
fn given_a_favorited_album_when_the_app_restarts_then_the_stale_snapshot_does_not_undo_it() {
    let library = library_of(SONGS);
    let destination = album_page();
    let first_run = visited(Arc::clone(&library), &destination);
    let state = first_run.app.global::<AlbumDetailState>();
    let favorited = !state.get_is_favorite();
    state.invoke_toggle_favorite();
    assert!(first_run.wait_until_idle(), "the edit reached the server");
    let held = held_on(library);
    let run = first_run.restart(served_by(&held));

    destination.open_in(&run);

    assert!(run.pump(|| destination.is_shown_in(&run)));
    assert_eq!(held.answers(), 0);
    let restored = run.app.global::<AlbumDetailState>();
    assert_eq!(restored.get_is_favorite(), favorited);
}

#[test]
fn given_a_cached_playlist_when_it_is_deleted_then_a_later_visit_finds_nothing_cached() {
    let library = library_of(SONGS);
    let playlist_id = created_playlist_id(&library);
    let destination = Destination {
        route: Route::PlaylistDetail,
        detail_id: playlist_id.clone(),
        is_shown: shows_playlist_detail,
    };
    let run = visited(library, &destination);
    let metadata = View::Playlist(playlist_id.clone()).slot("metadata");
    let store = run.context.store.views();
    assert!(block_on(store.stored::<Playlist>(DEMO_SERVER, &metadata)).is_some());
    let app = run.app.global::<AppState>();
    app.invoke_navigate(Route::Playlists, "".into());
    run.app
        .global::<PlaylistsState>()
        .invoke_delete(playlist_id.as_str().into());

    assert!(run.wait_until_idle(), "the deletion settles");

    assert!(block_on(store.stored::<Playlist>(DEMO_SERVER, &metadata)).is_none());
}

#[test]
fn given_a_cached_playlist_when_songs_are_added_then_its_stale_pages_are_dropped() {
    let library = library_of(SONGS);
    let playlist_id = created_playlist_id(&library);
    let destination = Destination {
        route: Route::PlaylistDetail,
        detail_id: playlist_id.clone(),
        is_shown: shows_playlist_detail,
    };
    let new_song = song_outside_the_playlist(&library);
    let run = visited(library, &destination);
    let first_page = View::Playlist(playlist_id.clone()).slot("first-page");
    let store = run.context.store.views();
    assert!(block_on(store.stored::<FirstPage>(DEMO_SERVER, &first_page)).is_some());
    run.app
        .global::<AppState>()
        .invoke_navigate(Route::Tracks, "".into());
    run.app
        .global::<Actions>()
        .invoke_add_to_playlist(ModelRc::new(VecModel::from(vec![new_song])));
    let picker = run.app.global::<PlaylistPickerState>();
    assert!(run.pump(|| !picker.get_is_loading()));

    picker.invoke_pick(playlist_id.as_str().into());

    assert!(run.pump_until_status("Added"));
    assert!(run.wait_until_idle(), "the addition settles");
    assert!(block_on(store.stored::<FirstPage>(DEMO_SERVER, &first_page)).is_none());
}

#[test]
fn given_a_cached_station_when_it_is_deleted_then_a_restart_does_not_bring_it_back() {
    let destination = Destination::page(Route::Radio, shows_stations);
    let library = library_of(SONGS);
    let run = visited(Arc::clone(&library), &destination);
    let state = run.app.global::<RadioState>();
    let stations = state.get_stations();
    let doomed = stations.row_data(0).unwrap();
    state.invoke_delete(doomed.id.clone());
    assert!(run.pump(|| state.get_stations().iter().all(|row| row.id != doomed.id)));
    assert!(run.wait_until_idle(), "the reload settles");
    let held = held_on(library);
    let restarted = run.restart(served_by(&held));

    destination.open_in(&restarted);

    let starts_empty_or_without_it = restarted.pump(|| {
        let state = restarted.app.global::<RadioState>();
        !state.get_is_stations_loading() || state.get_stations().row_count() > 0
    });
    assert!(starts_empty_or_without_it);
    let shown = restarted.app.global::<RadioState>().get_stations();
    assert!(shown.iter().all(|row| row.id != doomed.id));
}
