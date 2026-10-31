use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{Paging, SongListQuery};
use slint::{ComponentHandle, Model};

use super::held_server::HeldServer;
use super::session_window::SessionWindow;
use crate::demo::{DemoLibrary, FakeServer};
use crate::ui::{
    AlbumDetailState, AlbumsState, AppState, AppWindow, ArtistDetailState, ArtistsState,
    FavoritesState, FoldersState, GenresState, HomeState, PlaylistDetailState, PlaylistsState,
    RadioState, Route, TracksState,
};

pub(super) const SONGS: usize = 400;
const PLAYLIST_SONGS: usize = 5;

pub(super) struct Destination {
    pub(super) route: Route,
    pub(super) detail_id: String,
    pub(super) is_shown: fn(&AppWindow) -> bool,
}

impl Destination {
    pub(super) fn page(route: Route, is_shown: fn(&AppWindow) -> bool) -> Self {
        Self {
            route,
            detail_id: String::new(),
            is_shown,
        }
    }

    pub(super) fn open_in(&self, run: &SessionWindow) {
        run.app
            .global::<AppState>()
            .invoke_navigate(self.route, self.detail_id.as_str().into());
    }

    pub(super) fn is_shown_in(&self, run: &SessionWindow) -> bool {
        (self.is_shown)(&run.app)
    }
}

pub(super) fn library_of(songs: usize) -> Arc<FakeServer> {
    Arc::new(FakeServer::with_config(
        FakeServer::config_for_demo(),
        DemoLibrary::with_songs(songs),
    ))
}

fn library() -> Arc<FakeServer> {
    library_of(SONGS)
}

pub(super) fn block_on<Output>(work: impl Future<Output = Output>) -> Output {
    tokio::runtime::Runtime::new().unwrap().block_on(work)
}

pub(super) fn first_artist_id(library: &FakeServer) -> String {
    let album = block_on(library.album("album-0")).unwrap();
    album.album.album_artists.first().unwrap().id.clone()
}

pub(super) fn created_playlist_id(library: &FakeServer) -> String {
    let paging = Paging {
        start_index: 0,
        limit: PLAYLIST_SONGS,
    };
    let songs = block_on(library.song_list(&SongListQuery::default(), paging)).unwrap();
    let song_ids: Vec<String> = songs.items.into_iter().map(|song| song.id).collect();
    block_on(library.create_playlist("Road trip", &song_ids))
        .unwrap()
        .id
}

pub(super) fn shows_tracks(window: &AppWindow) -> bool {
    let rows = window.global::<TracksState>().get_rows();
    rows.row_data(0).is_some_and(|row| row.is_loaded)
}

pub(super) fn shows_home_albums(window: &AppWindow) -> bool {
    let shelves = window.global::<HomeState>().get_shelves();
    shelves.iter().any(|shelf| shelf.albums.row_count() > 0)
}

pub(super) fn shows_albums(window: &AppWindow) -> bool {
    let rows = window.global::<AlbumsState>().get_rows();
    let first_card = rows.row_data(0).and_then(|row| row.cards.row_data(0));
    first_card.is_some_and(|card| card.is_loaded)
}

pub(super) fn shows_album_detail(window: &AppWindow) -> bool {
    let state = window.global::<AlbumDetailState>();
    !state.get_name().is_empty() && state.get_tracks().row_count() > 0
}

pub(super) fn shows_artists(window: &AppWindow) -> bool {
    let items = window.global::<ArtistsState>().get_items();
    items.row_data(0).is_some_and(|card| !card.name.is_empty())
}

pub(super) fn shows_artist_detail(window: &AppWindow) -> bool {
    let state = window.global::<ArtistDetailState>();
    !state.get_name().is_empty()
        && state.get_albums().row_count() > 0
        && state.get_top_songs().row_count() > 0
}

fn shows_genres(window: &AppWindow) -> bool {
    window.global::<GenresState>().get_genres().row_count() > 0
}

fn shows_playlists(window: &AppWindow) -> bool {
    window.global::<PlaylistsState>().get_rows().row_count() > 0
}

pub(super) fn shows_playlist_detail(window: &AppWindow) -> bool {
    let state = window.global::<PlaylistDetailState>();
    let first_row = state.get_rows().row_data(0);
    !state.get_name().is_empty() && first_row.is_some_and(|row| row.track.is_loaded)
}

fn shows_favorites(window: &AppWindow) -> bool {
    let tracks = window.global::<FavoritesState>().get_tracks();
    tracks.row_data(0).is_some_and(|item| item.track.is_loaded)
}

fn shows_folders(window: &AppWindow) -> bool {
    window.global::<FoldersState>().get_rows().row_count() > 0
}

pub(super) fn shows_stations(window: &AppWindow) -> bool {
    window.global::<RadioState>().get_stations().row_count() > 0
}

pub(super) fn visited(library: Arc<FakeServer>, destination: &Destination) -> SessionWindow {
    let run = SessionWindow::open(library);
    destination.open_in(&run);
    assert!(
        run.pump(|| destination.is_shown_in(&run)),
        "the first run shows the view"
    );
    assert!(run.wait_until_idle(), "the first run settles");
    run
}

fn assert_shown_before_any_answer(library: Arc<FakeServer>, destination: &Destination) {
    let first_run = visited(Arc::clone(&library), destination);
    let held = Arc::new(HeldServer::new(library));
    let second_run = first_run.restart(Arc::clone(&held) as Arc<dyn MusicServer>);

    destination.open_in(&second_run);

    assert!(
        second_run.pump(|| destination.is_shown_in(&second_run)),
        "the cache shows the view after the restart"
    );
    assert_eq!(held.answers(), 0, "no server call was answered yet");
    held.release();
    assert!(second_run.pump(|| held.answers() > 0));
    assert!(second_run.wait_until_idle(), "the refresh settles");
    assert!(destination.is_shown_in(&second_run));
}

#[test]
fn given_visited_tracks_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Tracks, shows_tracks);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_a_visited_home_when_the_app_restarts_then_its_shelves_show_before_any_answer() {
    let destination = Destination::page(Route::Home, shows_home_albums);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_visited_albums_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Albums, shows_albums);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_a_visited_album_when_the_app_restarts_then_it_shows_before_any_answer() {
    let destination = Destination {
        route: Route::AlbumDetail,
        detail_id: "album-0".into(),
        is_shown: shows_album_detail,
    };
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_visited_artists_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Artists, shows_artists);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_a_visited_artist_when_the_app_restarts_then_it_shows_before_any_answer() {
    let library = library();
    let destination = Destination {
        route: Route::ArtistDetail,
        detail_id: first_artist_id(&library),
        is_shown: shows_artist_detail,
    };
    assert_shown_before_any_answer(library, &destination);
}

#[test]
fn given_visited_genres_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Genres, shows_genres);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_visited_playlists_when_the_app_restarts_then_they_show_before_any_answer() {
    let library = library();
    created_playlist_id(&library);
    let destination = Destination::page(Route::Playlists, shows_playlists);
    assert_shown_before_any_answer(library, &destination);
}

#[test]
fn given_a_visited_playlist_when_the_app_restarts_then_it_shows_before_any_answer() {
    let library = library();
    let destination = Destination {
        route: Route::PlaylistDetail,
        detail_id: created_playlist_id(&library),
        is_shown: shows_playlist_detail,
    };
    assert_shown_before_any_answer(library, &destination);
}

#[test]
fn given_visited_favorites_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Favorites, shows_favorites);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_visited_folders_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Folders, shows_folders);
    assert_shown_before_any_answer(library(), &destination);
}

#[test]
fn given_visited_radio_stations_when_the_app_restarts_then_they_show_before_any_answer() {
    let destination = Destination::page(Route::Radio, shows_stations);
    assert_shown_before_any_answer(library(), &destination);
}
