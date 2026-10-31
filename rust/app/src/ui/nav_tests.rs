use std::sync::Arc;

use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use slint::platform::PointerEventButton;

use super::detail_navigation_tests::{Loads, open_detail, settle};
use super::test_window::TestWindow;
use super::{AppState, AppWindow, PlayerState, Route};
use crate::demo::FakeServer;

const SONGS: usize = 3;

struct Browser {
    window: AppWindow,
    loads: Loads,
}

impl Browser {
    fn open() -> Self {
        let (window, loads) = open_detail(Route::Tracks, "");
        Self { window, loads }
    }

    fn go_to(&self, route: Route, detail_id: &str) {
        self.window
            .global::<AppState>()
            .invoke_navigate(route, detail_id.into());
        settle();
    }

    fn labelled(&self, element_type: &str, label: &str) -> ElementHandle {
        ElementHandle::find_by_element_type_name(&self.window, element_type)
            .find(|element| element.accessible_label().as_deref() == Some(label))
            .unwrap()
    }

    fn click(&self, element_type: &str, label: &str) {
        self.labelled(element_type, label)
            .mock_single_click(PointerEventButton::Left);
        settle();
    }

    fn click_sidebar(&self, label: &str) {
        self.click("NavEntry", label);
    }

    fn click_back(&self) {
        self.click("IconButton", "Back");
    }

    fn click_forward(&self) {
        self.click("IconButton", "Forward");
    }

    fn click_player_info(&self) {
        ElementHandle::find_by_element_id(&self.window, "PlayerBar::info-touch")
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        settle();
    }

    fn is_shown(&self, id: &str) -> bool {
        ElementHandle::find_by_element_id(&self.window, id)
            .next()
            .is_some()
    }

    fn location(&self) -> (Route, String) {
        location_of(&self.window)
    }

    fn arrows(&self) -> (bool, bool) {
        arrows_of(&self.window)
    }

    fn take_loads(&self) -> Vec<String> {
        self.loads.borrow_mut().drain(..).collect()
    }

    fn with_a_song(self) -> Self {
        self.window.global::<PlayerState>().set_has_song(true);
        settle();
        self
    }
}

pub(super) fn location_of(window: &AppWindow) -> (Route, String) {
    let state = window.global::<AppState>();
    (state.get_route(), state.get_detail_id().into())
}

pub(super) fn arrows_of(window: &AppWindow) -> (bool, bool) {
    let state = window.global::<AppState>();
    (state.get_can_go_back(), state.get_can_go_forward())
}

fn detail(route: Route, detail_id: &str) -> (Route, String) {
    (route, detail_id.to_owned())
}

pub(super) fn page(route: Route) -> (Route, String) {
    detail(route, "")
}

fn on_playlists_after_an_artist() -> Browser {
    let browser = Browser::open();
    browser.go_to(Route::PlaylistDetail, "playlist-1");
    browser.click_sidebar("Artists");
    browser.go_to(Route::ArtistDetail, "artist-1");
    browser.click_sidebar("Playlists");
    browser
}

#[test]
fn given_an_artist_left_for_playlists_when_clicking_back_then_that_artist_returns() {
    let browser = on_playlists_after_an_artist();

    browser.click_back();

    assert_eq!(browser.location(), detail(Route::ArtistDetail, "artist-1"));
}

#[test]
fn given_the_artist_returned_when_clicking_back_again_then_artists_returns() {
    let browser = on_playlists_after_an_artist();
    browser.click_back();

    browser.click_back();

    assert_eq!(browser.location(), page(Route::Artists));
}

#[test]
fn given_a_page_left_by_back_when_clicking_forward_then_it_is_entered_again() {
    let browser = on_playlists_after_an_artist();
    browser.click_back();
    browser.click_back();

    browser.click_forward();
    assert_eq!(browser.location(), detail(Route::ArtistDetail, "artist-1"));

    browser.click_forward();
    assert_eq!(browser.location(), page(Route::Playlists));
}

#[test]
fn given_a_back_when_navigating_elsewhere_then_forward_is_cleared() {
    let browser = on_playlists_after_an_artist();
    browser.click_back();
    assert_eq!(browser.arrows(), (true, true));

    browser.click_sidebar("Favorites");

    assert_eq!(browser.arrows(), (true, false));
    browser.window.global::<AppState>().invoke_forward();
    settle();
    assert_eq!(browser.location(), page(Route::Favorites));
}

#[test]
fn given_a_back_when_navigating_to_the_current_page_then_forward_is_kept() {
    let browser = on_playlists_after_an_artist();
    browser.click_back();
    browser.click_back();

    browser.click_sidebar("Artists");

    assert_eq!(browser.arrows(), (true, true));
}

#[test]
fn given_a_visit_when_moving_through_it_then_the_arrows_tell_what_is_reachable() {
    let browser = Browser::open();
    assert_eq!(browser.arrows(), (false, false));

    browser.click_sidebar("Artists");
    assert_eq!(browser.arrows(), (true, false));

    browser.go_to(Route::ArtistDetail, "artist-1");
    assert_eq!(browser.arrows(), (true, false));

    browser.click_back();
    assert_eq!(browser.arrows(), (true, true));

    browser.click_back();
    assert_eq!(browser.arrows(), (false, true));

    browser.click_forward();
    assert_eq!(browser.arrows(), (true, true));

    browser.click_forward();
    assert_eq!(browser.arrows(), (true, false));
}

#[test]
fn given_nowhere_to_go_when_clicking_the_arrows_then_nothing_moves() {
    let browser = Browser::open();

    browser.click("IconButton", "Nothing to go back to");
    browser.click("IconButton", "Nothing to go forward to");

    assert_eq!(browser.location(), page(Route::Tracks));
    assert_eq!(browser.arrows(), (false, false));
}

#[test]
fn given_the_arrows_when_history_changes_then_they_are_disabled_only_with_nowhere_to_go() {
    let browser = Browser::open();
    let is_enabled = |label| browser.labelled("IconButton", label).accessible_enabled();
    assert_eq!(is_enabled("Nothing to go back to"), Some(false));
    assert_eq!(is_enabled("Nothing to go forward to"), Some(false));

    browser.click_sidebar("Artists");
    assert_eq!(is_enabled("Back"), Some(true));

    browser.click_back();
    assert_eq!(is_enabled("Forward"), Some(true));
}

#[test]
fn given_any_page_when_it_opens_then_both_arrows_stay_on_screen() {
    let browser = Browser::open();
    for route in [
        Route::Home,
        Route::Albums,
        Route::Artists,
        Route::Genres,
        Route::Playlists,
        Route::Favorites,
        Route::Search,
        Route::Settings,
        Route::NowPlaying,
        Route::Folders,
        Route::Radio,
        Route::Discover,
    ] {
        browser.go_to(route, "");
        assert!(browser.is_shown("Sidebar::back-arrow"), "{route:?}");
        assert!(browser.is_shown("Sidebar::forward-arrow"), "{route:?}");
    }
}

fn right_edge(element: &ElementHandle) -> f32 {
    element.absolute_position().x + element.size().width
}

#[test]
fn given_the_real_font_when_laid_out_then_the_arrows_fit_and_leave_the_menu_in_place() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(SONGS))) else {
        return;
    };
    let by_label = |label| {
        ElementHandle::find_by_accessible_label(&view.app, label)
            .next()
            .unwrap()
    };
    let by_id = |id| {
        ElementHandle::find_by_element_id(&view.app, id)
            .next()
            .unwrap()
    };
    let title = by_label("Cadence");
    let menu_label = by_label("LIBRARY");
    let back = by_id("Sidebar::back-arrow");
    let forward = by_id("Sidebar::forward-arrow");

    assert!(right_edge(&title) <= back.absolute_position().x);
    assert!(right_edge(&forward) <= right_edge(&menu_label));
    assert_eq!(
        menu_label.absolute_position().y - title.absolute_position().y,
        42.0
    );
}

fn assert_returning_loads_once(route: Route, detail_id: &str) {
    let browser = Browser::open();
    browser.go_to(route, detail_id);
    browser.click_sidebar("Favorites");
    assert_eq!(browser.take_loads(), [detail_id]);

    browser.click_back();
    assert_eq!(browser.location(), detail(route, detail_id));
    assert_eq!(browser.take_loads(), [detail_id]);

    browser.click_back();
    assert_eq!(browser.take_loads(), Vec::<String>::new());

    browser.click_forward();
    assert_eq!(browser.take_loads(), [detail_id]);
}

#[test]
fn given_an_artist_in_history_when_returning_then_that_artist_loads_once() {
    assert_returning_loads_once(Route::ArtistDetail, "artist-1");
}

#[test]
fn given_an_album_in_history_when_returning_then_that_album_loads_once() {
    assert_returning_loads_once(Route::AlbumDetail, "album-1");
}

#[test]
fn given_a_playlist_in_history_when_returning_then_that_playlist_loads_once() {
    assert_returning_loads_once(Route::PlaylistDetail, "playlist-1");
}

#[test]
fn given_two_artists_in_a_row_when_moving_between_them_then_each_loads_once() {
    let browser = Browser::open();
    browser.go_to(Route::ArtistDetail, "artist-1");
    browser.go_to(Route::ArtistDetail, "artist-2");
    assert_eq!(browser.take_loads(), ["artist-1", "artist-2"]);

    browser.click_back();
    assert_eq!(browser.take_loads(), ["artist-1"]);

    browser.click_forward();
    assert_eq!(browser.take_loads(), ["artist-2"]);
}

#[test]
fn given_a_detail_page_of_another_kind_when_returning_then_only_that_page_loads() {
    let browser = Browser::open();
    browser.go_to(Route::AlbumDetail, "album-1");
    browser.go_to(Route::ArtistDetail, "artist-1");
    assert_eq!(browser.take_loads(), ["album-1", "artist-1"]);

    browser.click_back();

    assert_eq!(browser.take_loads(), ["album-1"]);
}

#[test]
fn given_now_playing_opened_from_a_page_when_closing_it_then_that_page_returns() {
    let browser = Browser::open().with_a_song();
    browser.click_sidebar("Albums");
    browser.click_player_info();
    assert_eq!(browser.location(), page(Route::NowPlaying));

    browser.click_player_info();

    assert_eq!(browser.location(), page(Route::Albums));
    assert_eq!(browser.arrows(), (true, true));
    browser.click_forward();
    assert_eq!(browser.location(), page(Route::NowPlaying));
}

#[test]
fn given_a_forward_history_when_closing_now_playing_then_it_is_kept() {
    let browser = Browser::open().with_a_song();
    browser.click_player_info();
    browser.click_sidebar("Albums");
    browser.click_back();
    assert_eq!(browser.location(), page(Route::NowPlaying));

    browser.click_player_info();
    assert_eq!(browser.location(), page(Route::Tracks));

    browser.click_forward();
    browser.click_forward();
    assert_eq!(browser.location(), page(Route::Albums));
}

#[test]
fn given_lyrics_opened_from_a_page_when_hiding_them_then_that_page_returns() {
    let browser = Browser::open().with_a_song();
    browser.click_sidebar("Albums");
    browser.click("IconButton", "Show lyrics");
    assert_eq!(browser.location(), page(Route::NowPlaying));

    browser.click("IconButton", "Hide lyrics");

    assert_eq!(browser.location(), page(Route::Albums));
    assert_eq!(browser.arrows(), (true, true));
}
