use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{ArtistListQuery, Paging};
use slint::{ComponentHandle, Model, SharedString};

use super::list::PAGE_SIZE;
use super::query::build_query;
use super::requests::page_slot;
use crate::demo::FakeServer;
use crate::tests::held_server::HeldServer;
use crate::tests::session_window::SessionWindow;
use crate::ui::test_window::TestWindow;
use crate::ui::{AppState, ArtistsState, ListControls, Route};

const STALE_NAME: &str = "Stale cached artist";

fn navigate(view: &TestWindow, route: Route) {
    view.app
        .global::<AppState>()
        .invoke_navigate(route, SharedString::default());
}

fn is_loaded(view: &TestWindow) -> bool {
    let state = view.app.global::<ArtistsState>();
    !state.get_is_loading() && state.get_items().row_count() > 0
}

#[test]
fn given_the_artists_page_is_opened_again_it_loads_the_artists_again() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(200))) else {
        return;
    };
    navigate(&view, Route::Artists);
    assert!(view.pump(|| is_loaded(&view)));
    navigate(&view, Route::Tracks);
    assert!(view.pump(|| true));

    navigate(&view, Route::Artists);

    assert!(view.pump(|| view.app.global::<ArtistsState>().get_is_loading()));
    assert!(view.pump(|| is_loaded(&view)));
}

fn opened_artists_query() -> ArtistListQuery {
    build_query(&ListControls {
        sort_index: 0,
        is_descending: false,
        search: "",
        is_favorites_only: false,
    })
}

#[test]
fn given_a_stale_cached_page_when_the_refresh_answers_then_rows_change_in_the_same_model() {
    let library = Arc::new(FakeServer::new(200));
    let held = Arc::new(HeldServer::new(Arc::clone(&library) as Arc<dyn MusicServer>));
    let run = SessionWindow::open(Arc::clone(&held) as Arc<dyn MusicServer>);
    let query = opened_artists_query();
    let paging = Paging {
        start_index: 0,
        limit: PAGE_SIZE,
    };
    let answers = run
        .context
        .runtime
        .block_on(library.artist_list(&query, paging));
    let mut stale = answers.unwrap().items;
    let fresh_name = stale.first().unwrap().name.clone();
    stale.first_mut().unwrap().name = STALE_NAME.into();
    let server_id = FakeServer::config_for_demo().id;
    let slot = page_slot(&query, 0).unwrap();
    let store = run.context.store.views();
    run.context
        .runtime
        .block_on(store.seed(&server_id, &slot, stale));
    let state = run.app.global::<ArtistsState>();
    run.app
        .global::<AppState>()
        .invoke_navigate(Route::Artists, SharedString::default());
    let first_name = || state.get_items().row_data(0).map(|card| card.name);
    assert!(run.pump(|| first_name().is_some_and(|name| name == STALE_NAME)));
    let model = state.get_items();
    let rows = model.row_count();

    held.release();

    assert!(run.pump(|| first_name().is_some_and(|name| name == fresh_name.as_str())));
    assert!(state.get_items() == model, "the rows stay in their model");
    assert_eq!(model.row_count(), rows);
    assert!(!state.get_is_loading());
}
