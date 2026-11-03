use std::cell::Cell;
use std::sync::Arc;

use i_slint_backend_testing::{ElementHandle, ElementQuery};
use slint::platform::WindowEvent;
use slint::{ComponentHandle, LogicalPosition};

use super::test_window::{SIZE, TestWindow};
use super::{AppState, Route};
use crate::demo::FakeServer;

const SONGS: usize = 2_000;
const SCROLL_POINT: LogicalPosition = LogicalPosition::new(700.0, 600.0);
const SCROLL_DOWN: f32 = -2_350.0;
const FIRST_TRACK: &str = "1";
const SETTLE_FRAMES: u32 = 20;

type Snapshot = Vec<(f32, Option<String>)>;

struct Page {
    route: Route,
    detail_id: &'static str,
    marker: &'static str,
}

const FIRST_ARTIST: Page = Page {
    route: Route::ArtistDetail,
    detail_id: "artist-1",
    marker: "ArtistImage",
};

const SECOND_ARTIST: Page = Page {
    route: Route::ArtistDetail,
    detail_id: "artist-2",
    marker: "ArtistImage",
};

const LIBRARY_PAGES: [Page; 5] = [
    Page {
        route: Route::Home,
        detail_id: "",
        marker: "ShelfView",
    },
    Page {
        route: Route::Albums,
        detail_id: "",
        marker: "AlbumGridCardView",
    },
    Page {
        route: Route::Artists,
        detail_id: "",
        marker: "ArtistCardView",
    },
    Page {
        route: Route::PlaylistDetail,
        detail_id: "playlist-1",
        marker: "SongRow",
    },
    Page {
        route: Route::Settings,
        detail_id: "",
        marker: "Section",
    },
];

struct Library {
    view: TestWindow,
}

impl Library {
    fn open() -> Option<Self> {
        let view = TestWindow::open(Arc::new(FakeServer::new(SONGS)))?;
        let library = Self { view };
        assert!(library.view.pump(|| library.first_shown_track().is_some()));
        Some(library)
    }

    fn state(&self) -> AppState<'_> {
        self.view.app.global::<AppState>()
    }

    fn go_to(&self, route: Route, detail_id: &str) {
        self.state().invoke_navigate(route, detail_id.into());
        self.settle();
    }

    fn back(&self) {
        self.state().invoke_back();
        self.settle();
    }

    fn forward(&self) {
        self.state().invoke_forward();
        self.settle();
    }

    fn scroll_down(&self) {
        self.view
            .app
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: SCROLL_POINT,
                delta_x: 0.0,
                delta_y: SCROLL_DOWN,
            });
        self.settle();
    }

    fn settle(&self) {
        let frames = Cell::new(0);
        self.view.pump(|| {
            frames.set(frames.get() + 1);
            frames.get() > SETTLE_FRAMES
        });
    }

    fn by_type(&self, type_name: &str) -> Vec<ElementHandle> {
        ElementQuery::from_root(&self.view.app)
            .match_descendants()
            .match_type_name(type_name)
            .find_all()
    }

    fn track_rows(&self) -> usize {
        self.by_type("TrackRowView").len()
    }

    fn list_top(&self) -> Option<f32> {
        let header = self.by_type("TracksHeader").into_iter().next()?;
        Some(header.absolute_position().y + header.size().height)
    }

    fn first_shown_track(&self) -> Option<String> {
        let list_top = self.list_top()?;
        self.by_type("TrackIndexCell")
            .into_iter()
            .filter(|cell| cell.absolute_position().y >= list_top)
            .filter_map(|cell| Some((cell.absolute_position().y, text_label(&cell)?)))
            .min_by(|(first_y, _), (second_y, _)| first_y.total_cmp(second_y))
            .map(|(_, label)| label)
    }

    fn shows_first_track_again(&self, expected: Option<String>) -> bool {
        self.view.pump(|| self.first_shown_track() == expected)
    }

    fn scrolled_tracks(&self) -> Option<String> {
        self.scroll_down();
        let shown = self.first_shown_track();
        assert_ne!(shown.as_deref(), Some(FIRST_TRACK));
        assert!(shown.is_some());
        shown
    }

    fn on_screen(&self, page: &Page) -> Snapshot {
        let mut shown: Snapshot = self
            .by_type(page.marker)
            .iter()
            .filter(|element| is_on_screen(element))
            .map(|element| (element.absolute_position().y, label_of(element)))
            .collect();
        shown.sort_by(|first, second| first.0.total_cmp(&second.0).then(first.1.cmp(&second.1)));
        shown.dedup();
        shown
    }

    fn is_back_at(&self, page: &Page, scrolled: &Snapshot) -> bool {
        self.view.pump(|| self.on_screen(page) == *scrolled)
    }

    fn scrolled_page(&self, page: &Page) -> Snapshot {
        self.go_to(page.route, page.detail_id);
        assert!(self.view.pump(|| !self.on_screen(page).is_empty()));
        let top = self.on_screen(page);
        self.scroll_down();
        let scrolled = self.on_screen(page);
        assert_ne!(scrolled, top, "{:?} did not scroll", page.route);
        assert!(
            !scrolled.is_empty(),
            "{:?} scrolled out of view",
            page.route
        );
        scrolled
    }
}

fn is_on_screen(element: &ElementHandle) -> bool {
    let top = element.absolute_position().y;
    let bottom = top + element.size().height;
    bottom > 0.0 && f64::from(top) < f64::from(SIZE.height)
}

fn text_label(element: &ElementHandle) -> Option<String> {
    element
        .query_descendants()
        .match_type_name("Text")
        .find_first()?
        .accessible_label()
        .map(String::from)
}

fn label_of(element: &ElementHandle) -> Option<String> {
    element
        .accessible_label()
        .map(String::from)
        .filter(|label| !label.is_empty())
        .or_else(|| text_label(element))
}

#[test]
fn given_tracks_scrolled_down_when_going_back_from_an_artist_then_the_same_tracks_are_shown() {
    let Some(library) = Library::open() else {
        return;
    };
    let shown = library.scrolled_tracks();

    library.go_to(Route::ArtistDetail, "artist-1");
    library.back();

    assert!(library.shows_first_track_again(shown));
}

#[test]
fn given_tracks_scrolled_down_when_opening_them_from_the_sidebar_then_they_start_at_the_top() {
    let Some(library) = Library::open() else {
        return;
    };
    library.scrolled_tracks();

    library.go_to(Route::Albums, "");
    library.go_to(Route::Tracks, "");

    assert!(library.shows_first_track_again(Some(FIRST_TRACK.to_owned())));
}

#[test]
fn given_tracks_left_by_back_when_going_forward_to_them_then_the_same_tracks_are_shown() {
    let Some(library) = Library::open() else {
        return;
    };
    library.go_to(Route::Albums, "");
    library.go_to(Route::Tracks, "");
    let shown = library.scrolled_tracks();

    library.back();
    library.forward();

    assert!(library.shows_first_track_again(shown));
}

#[test]
fn given_a_long_track_list_when_returning_to_a_deep_position_then_no_more_rows_exist_than_at_the_top()
 {
    let Some(library) = Library::open() else {
        return;
    };
    let rows_at_top = library.track_rows();
    let shown = library.scrolled_tracks();

    library.go_to(Route::ArtistDetail, "artist-1");
    library.back();

    assert!(library.shows_first_track_again(shown));
    assert!(library.track_rows() <= rows_at_top);
    assert!(rows_at_top < SONGS);
}

#[test]
fn given_an_artist_scrolled_down_when_going_back_to_it_then_it_is_where_it_was() {
    let Some(library) = Library::open() else {
        return;
    };
    let scrolled = library.scrolled_page(&FIRST_ARTIST);

    library.go_to(Route::Albums, "");
    library.back();

    assert!(library.is_back_at(&FIRST_ARTIST, &scrolled));
}

#[test]
fn given_an_artist_scrolled_down_when_opening_another_artist_then_it_starts_at_the_top() {
    let Some(library) = Library::open() else {
        return;
    };
    library.go_to(SECOND_ARTIST.route, SECOND_ARTIST.detail_id);
    assert!(
        library
            .view
            .pump(|| !library.on_screen(&SECOND_ARTIST).is_empty())
    );
    let top = library.on_screen(&SECOND_ARTIST);
    library.scrolled_page(&FIRST_ARTIST);

    library.go_to(SECOND_ARTIST.route, SECOND_ARTIST.detail_id);

    assert!(library.is_back_at(&SECOND_ARTIST, &top));
}

#[test]
fn given_an_artist_scrolled_down_when_going_back_from_another_artist_then_it_is_where_it_was() {
    let Some(library) = Library::open() else {
        return;
    };
    let scrolled = library.scrolled_page(&FIRST_ARTIST);

    library.go_to(SECOND_ARTIST.route, SECOND_ARTIST.detail_id);
    library.back();

    assert!(library.is_back_at(&FIRST_ARTIST, &scrolled));
}

#[test]
fn given_a_library_page_scrolled_down_when_going_back_to_it_then_it_is_where_it_was() {
    let Some(library) = Library::open() else {
        return;
    };
    for page in &LIBRARY_PAGES {
        let scrolled = library.scrolled_page(page);

        library.go_to(Route::Tracks, "");
        library.back();

        assert!(
            library.is_back_at(page, &scrolled),
            "{:?} lost its place",
            page.route
        );
    }
}
