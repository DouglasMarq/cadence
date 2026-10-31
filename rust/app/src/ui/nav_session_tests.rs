use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::ServerConfig;
use cadence_storage::settings::Settings;
use slint::{ComponentHandle, Model};

use super::nav_tests::{arrows_of, location_of, page};
use super::test_window::TestWindow;
use super::{AppState, LoginState, Page, Route, SettingsState, login};
use crate::controller::{Command, SettingsSnapshot};
use crate::demo::{DemoLibrary, FakeServer, demo_credential};

const SONGS: usize = 3;
const OTHER_SERVER_ID: &str = "other";
const OTHER_SERVER_NAME: &str = "Other server";

fn other_config() -> ServerConfig {
    ServerConfig {
        id: OTHER_SERVER_ID.into(),
        name: OTHER_SERVER_NAME.into(),
        ..FakeServer::config_for_demo()
    }
}

fn other_server() -> Arc<dyn MusicServer> {
    Arc::new(FakeServer::with_config(
        other_config(),
        DemoLibrary::with_songs(SONGS),
    ))
}

fn open_session() -> Option<TestWindow> {
    TestWindow::open(Arc::new(FakeServer::new(SONGS)))
}

fn browse(view: &TestWindow) {
    let state = view.app.global::<AppState>();
    state.invoke_navigate(Route::ArtistDetail, "artist-0".into());
    state.invoke_navigate(Route::Settings, "".into());
    state.invoke_navigate(Route::AlbumDetail, "album-0".into());
    state.invoke_back();
    assert_eq!(location_of(&view.app), page(Route::Settings));
    assert_eq!(arrows_of(&view.app), (true, true));
}

fn assert_no_way_to_the_previous_session(view: &TestWindow) {
    let state = view.app.global::<AppState>();
    let landing = location_of(&view.app);
    assert_eq!(arrows_of(&view.app), (false, false));

    state.invoke_back();
    state.invoke_forward();

    assert_eq!(location_of(&view.app), landing);
}

fn show_known_servers(view: &TestWindow) {
    let settings = view.app.global::<SettingsState>();
    let reply = std::iter::from_fn(|| view.next_command()).find_map(|command| match command {
        Command::GetSettings(reply) => Some(reply),
        _ => None,
    });
    let snapshot = SettingsSnapshot {
        settings: Settings {
            servers: vec![FakeServer::config_for_demo(), other_config()],
            current_server_id: Some(FakeServer::config_for_demo().id),
            ..Settings::default()
        },
        devices: Vec::new(),
    };
    assert!(reply.unwrap().send(snapshot).is_ok());
    assert!(view.pump(|| settings.get_servers().row_count() == 2));
}

fn sign_in(view: &TestWindow) {
    let state = view.app.global::<AppState>();
    assert_eq!(state.get_page(), Page::Login);
    let form = view.app.global::<LoginState>();
    form.set_url("music.example.com".into());
    form.set_username("ada".into());
    form.set_password("secret".into());
    form.invoke_login();
    assert!(view.pump(|| state.get_page() == Page::Main));
}

#[test]
fn given_a_history_when_entering_another_server_then_it_is_gone() {
    let Some(view) = open_session() else {
        return;
    };
    browse(&view);

    login::enter_main(&view.app, &other_server());

    assert_eq!(location_of(&view.app), page(Route::Settings));
    assert_no_way_to_the_previous_session(&view);
}

#[test]
fn given_a_detail_page_open_when_entering_another_server_then_it_is_left_for_tracks() {
    let Some(view) = open_session() else {
        return;
    };
    let state = view.app.global::<AppState>();
    state.invoke_navigate(Route::ArtistDetail, "artist-0".into());

    login::enter_main(&view.app, &other_server());

    assert_eq!(location_of(&view.app), page(Route::Tracks));
    assert_no_way_to_the_previous_session(&view);
    state.invoke_navigate(Route::Albums, "".into());
    state.invoke_back();
    assert_eq!(location_of(&view.app), page(Route::Tracks));
}

#[test]
fn given_now_playing_open_when_entering_another_server_then_it_is_left_for_tracks() {
    let Some(view) = open_session() else {
        return;
    };
    view.app
        .global::<AppState>()
        .invoke_navigate(Route::NowPlaying, "".into());

    login::enter_main(&view.app, &other_server());

    assert_eq!(location_of(&view.app), page(Route::Tracks));
    assert_no_way_to_the_previous_session(&view);
}

#[test]
fn given_a_history_when_logging_out_and_signing_in_again_then_it_is_gone() {
    let Some(view) = open_session() else {
        return;
    };
    browse(&view);

    view.app.global::<SettingsState>().invoke_log_out();
    sign_in(&view);

    assert_no_way_to_the_previous_session(&view);
}

#[test]
fn given_a_history_when_signing_in_to_an_added_server_then_it_is_gone() {
    let Some(view) = open_session() else {
        return;
    };
    browse(&view);

    view.app.global::<SettingsState>().invoke_add_server();
    sign_in(&view);

    assert_no_way_to_the_previous_session(&view);
}

#[test]
fn given_a_history_when_switching_server_then_it_is_gone() {
    let Some(view) = open_session() else {
        return;
    };
    show_known_servers(&view);
    let saved = view
        .context
        .store
        .set_credential(OTHER_SERVER_ID, &demo_credential());
    assert!(saved.is_ok());
    browse(&view);

    view.app
        .global::<SettingsState>()
        .invoke_switch_server(OTHER_SERVER_ID.into());

    let state = view.app.global::<AppState>();
    assert!(view.pump(|| state.get_server_name() == OTHER_SERVER_NAME));
    assert_no_way_to_the_previous_session(&view);
}

#[test]
fn given_a_history_when_a_server_switch_fails_then_it_is_kept() {
    let Some(view) = open_session() else {
        return;
    };
    show_known_servers(&view);
    browse(&view);

    view.app
        .global::<SettingsState>()
        .invoke_switch_server(OTHER_SERVER_ID.into());

    assert!(view.pump(|| view.status().contains("Could not switch server")));
    assert_eq!(arrows_of(&view.app), (true, true));
}
