use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cadence_api::{Credential, MusicServer};
use cadence_core::model::{ArtistDetail, ServerConfig, ServerType};
use slint::{ComponentHandle, Model};
use tokio::runtime::Runtime;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::session_window::SessionWindow;
use crate::controller::Command;
use crate::covers::CachedCovers;
use crate::ui::test_window::TestWindow;
use crate::ui::{AppState, ArtistDetailState, ArtistsState, Route};
use crate::views::View;

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x56, 0xC7, 0x2F, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];
const NEVER_IN_THIS_TEST: Duration = Duration::from_secs(120);
const COVER_DELAY: Duration = Duration::from_millis(400);
const ARTISTS: usize = 1_000;
const BURST_ROWS: usize = 200;
const CONCURRENT_COVER_DOWNLOADS: usize = 6;
const SETTLE: Duration = Duration::from_millis(900);
const SERVER_ID: &str = "slow-server";
const FIXTURE_TOKEN: &str = "fixture-token";
const FIXTURE_SALT: &str = "fixture-salt";
const FIXTURE_USER: &str = "fixture-listener";

fn envelope(body: &str) -> String {
    format!(r#"{{"subsonic-response":{{"status":"ok","version":"1.16.1"{body}}}}}"#)
}

fn artists_body(count: usize) -> String {
    let artists: Vec<String> = (0..count)
        .map(|index| {
            format!(
                r#"{{"id":"ar-{index}","name":"Artist {index:05}","albumCount":3,"coverArt":"ar-{index}"}}"#
            )
        })
        .collect();
    let joined = artists.join(",");
    envelope(&format!(
        r#","artists":{{"index":[{{"name":"A","artist":[{joined}]}}]}}"#
    ))
}

fn artist_body() -> String {
    let albums: Vec<String> = (0..3)
        .map(|index| {
            format!(
                r#"{{"id":"al-{index}","name":"Album {index}","artist":"Slow Artist","artistId":"ar-1","coverArt":"al-{index}","songCount":10,"duration":1800,"created":"2024-05-01T10:00:00Z"}}"#
            )
        })
        .collect();
    let joined = albums.join(",");
    envelope(&format!(
        r#","artist":{{"id":"ar-1","name":"Slow Artist","albumCount":3,"coverArt":"ar-1","album":[{joined}]}}"#
    ))
}

fn top_songs_body() -> String {
    envelope(
        r#","topSongs":{"song":[{"id":"so-1","title":"Top Song","album":"Album 0","albumId":"al-0","artist":"Slow Artist","artistId":"ar-1","duration":180,"isDir":false}]}"#,
    )
}

fn json_answer(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json")
}

fn cover_answer() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(PNG_1X1.to_vec(), "image/png")
}

struct SlowServer {
    runtime: Runtime,
    mock: MockServer,
}

impl SlowServer {
    fn start() -> Self {
        let runtime = Runtime::new().unwrap();
        let mock = runtime.block_on(MockServer::start());
        let server = Self { runtime, mock };
        server.answer_rest();
        server
    }

    fn answer(&self, endpoint: &str, answer: ResponseTemplate) {
        let route = format!("/rest/{endpoint}.view");
        self.runtime.block_on(
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(answer)
                .mount(&self.mock),
        );
    }

    fn answer_rest(&self) {
        for endpoint in ["getGenres", "search3", "ping", "getOpenSubsonicExtensions"] {
            self.answer(endpoint, json_answer(envelope("")));
        }
    }

    fn client(&self) -> Arc<dyn MusicServer> {
        let config = ServerConfig {
            id: SERVER_ID.into(),
            name: "Slow".into(),
            server_type: ServerType::Subsonic,
            url: self.mock.uri(),
            username: FIXTURE_USER.into(),
        };
        let credential = Credential::Subsonic {
            subsonic: format!("u={FIXTURE_USER}&t={FIXTURE_TOKEN}&s={FIXTURE_SALT}"),
        };
        let server = cadence_api_subsonic::SubsonicServer::with_credential(config, &credential);
        Arc::new(server.unwrap())
    }

    fn requests_for(&self, endpoint: &str) -> usize {
        let requests = self.runtime.block_on(self.mock.received_requests());
        requests
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().contains(endpoint))
            .count()
    }
}

struct RealCovers {
    view: TestWindow,
    _directory: tempfile::TempDir,
}

fn open_with_real_covers(server: &SlowServer) -> Option<RealCovers> {
    let directory = tempfile::tempdir().unwrap();
    let paths = cadence_storage::Paths::under(directory.path());
    let covers = CachedCovers::new(&paths).unwrap();
    let view = TestWindow::open_with_covers(server.client(), Arc::new(covers))?;
    Some(RealCovers {
        view,
        _directory: directory,
    })
}

fn open_artist(view: &TestWindow) {
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::ArtistDetail, "ar-1".into());
}

#[test]
fn given_a_slow_image_biography_and_top_songs_when_an_artist_opens_then_nothing_waits_for_them() {
    let server = SlowServer::start();
    server.answer("getArtist", json_answer(artist_body()));
    for slow in ["getTopSongs", "getArtistInfo2"] {
        server.answer(
            slow,
            json_answer(envelope("")).set_delay(NEVER_IN_THIS_TEST),
        );
    }
    server.answer("getCoverArt", cover_answer().set_delay(NEVER_IN_THIS_TEST));
    server.answer("star", json_answer(envelope("")));
    let Some(opened) = open_with_real_covers(&server) else {
        return;
    };
    let view = &opened.view;

    open_artist(view);

    let state = view.app.global::<ArtistDetailState>();
    assert!(view.pump(|| state.get_name() == "Slow Artist"));
    assert!(view.pump(|| state.get_albums().row_count() == 3));
    assert!(state.get_is_loading_top_songs());
    assert!(state.get_is_loading_biography());
    assert!(!state.get_is_loading_albums());
    assert!(!state.get_has_image());
    state.invoke_toggle_favorite();
    assert!(view.pump(|| state.get_is_favorite()));
    state.invoke_play(false);
    let mut commands = std::iter::from_fn(|| view.next_command());
    assert!(commands.any(|command| matches!(command, Command::PlayFromList { .. })));
}

#[test]
fn given_only_a_slow_biography_when_an_artist_opens_then_the_top_songs_still_appear() {
    let server = SlowServer::start();
    server.answer("getArtist", json_answer(artist_body()));
    server.answer("getTopSongs", json_answer(top_songs_body()));
    server.answer(
        "getArtistInfo2",
        json_answer(envelope("")).set_delay(NEVER_IN_THIS_TEST),
    );
    server.answer("getCoverArt", cover_answer());
    let Some(opened) = open_with_real_covers(&server) else {
        return;
    };
    let view = &opened.view;

    open_artist(view);

    let state = view.app.global::<ArtistDetailState>();
    assert!(view.pump(|| state.get_top_songs().row_count() == 1));
    assert!(state.get_is_loading_biography());
    assert!(!state.get_is_loading_top_songs());
}

fn request_cover_rows(view: &TestWindow) {
    let state = view.app.global::<ArtistsState>();
    assert!(view.pump(|| state.get_items().row_count() >= BURST_ROWS / 2));
    let items = state.get_items();
    for index in 0..BURST_ROWS / 2 {
        items.row_data(index);
    }
    assert!(view.pump(|| state.get_items().row_count() >= BURST_ROWS));
    let items = state.get_items();
    for index in BURST_ROWS / 2..BURST_ROWS {
        items.row_data(index);
    }
}

#[test]
fn given_a_burst_of_covers_when_an_artist_is_clicked_then_it_is_answered_ahead_of_them() {
    let server = SlowServer::start();
    server.answer("getArtists", json_answer(artists_body(ARTISTS)));
    server.answer("getArtist", json_answer(artist_body()));
    server.answer("getCoverArt", cover_answer().set_delay(COVER_DELAY));
    server.answer("getTopSongs", json_answer(top_songs_body()));
    server.answer("getArtistInfo2", json_answer(envelope("")));
    let Some(opened) = open_with_real_covers(&server) else {
        return;
    };
    let view = &opened.view;
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::Artists, "".into());
    request_cover_rows(view);

    open_artist(view);

    let state = view.app.global::<ArtistDetailState>();
    assert!(view.pump(|| state.get_albums().row_count() == 3));
    let ahead_of_the_artist = server.requests_for("getCoverArt");
    assert!(
        ahead_of_the_artist <= 2 * CONCURRENT_COVER_DOWNLOADS,
        "{ahead_of_the_artist} covers went out before the artist answered"
    );
    let started = Instant::now();
    view.pump(|| started.elapsed() > SETTLE);
    let album_and_artist_images = 4;
    let most_covers = 2 * CONCURRENT_COVER_DOWNLOADS + album_and_artist_images;
    let total = server.requests_for("getCoverArt");
    assert!(
        total <= most_covers,
        "{total} covers were fetched for a page the user left"
    );
}

#[test]
fn given_visited_views_when_cached_then_no_credential_reaches_the_disk() {
    let server = SlowServer::start();
    server.answer("getArtists", json_answer(artists_body(3)));
    server.answer("getArtist", json_answer(artist_body()));
    server.answer("getTopSongs", json_answer(top_songs_body()));
    server.answer("getArtistInfo2", json_answer(envelope("")));
    server.answer("getCoverArt", cover_answer());
    let run = SessionWindow::open(server.client());
    let app = run.app.global::<AppState>();
    app.invoke_navigate(Route::Artists, "".into());
    let artists = run.app.global::<ArtistsState>();
    assert!(run.pump(|| artists.get_items().row_count() == 3));
    app.invoke_navigate(Route::ArtistDetail, "ar-1".into());
    let state = run.app.global::<ArtistDetailState>();
    assert!(run.pump(|| state.get_top_songs().row_count() == 1));
    let store = run.context.store.views();
    let slot = View::Artist("ar-1".into()).slot("detail");
    let is_stored = || {
        let stored = store.stored::<ArtistDetail>(SERVER_ID, &slot);
        run.context.runtime.block_on(stored).is_some()
    };
    assert!(run.pump(is_stored), "the artist snapshot was written");

    let entries: Vec<_> = fs::read_dir(run.cache_directory())
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();

    assert!(entries.len() >= 2, "the visited views were cached");
    for entry in entries {
        assert!(!entry.contains(FIXTURE_TOKEN));
        assert!(!entry.contains(FIXTURE_SALT));
        assert!(!entry.contains(FIXTURE_USER));
    }
}
