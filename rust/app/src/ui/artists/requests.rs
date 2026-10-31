use std::rc::Rc;
use std::sync::Arc;

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::{Artist, ArtistListQuery, Paging};
use slint::{ComponentHandle, Model, Weak};

use super::super::images::{CoverSource, to_image};
use super::super::{AppWindow, ArtistsState, Context, on_ui, show_status};
use super::item::ArtistItem;
use super::list::{ArtistList, PAGE_SIZE, Request};
use super::models::ListModel;
use super::query::build_query;
use crate::covers::CoverProvider;
use crate::ui::ListControls;
use crate::views::{View, ViewSlot};

const COVER_SIZE: u32 = 200;

fn shared_list(window: &AppWindow) -> Option<Rc<ArtistList>> {
    window
        .global::<ArtistsState>()
        .get_items()
        .as_any()
        .downcast_ref::<ListModel>()
        .map(|model| Rc::clone(&model.list))
}

pub fn with_list<Output>(
    window: &AppWindow,
    action: impl FnOnce(&ArtistList) -> Output,
) -> Option<Output> {
    shared_list(window).map(|list| action(&list))
}

fn finish_loading(window: &AppWindow, list: &ArtistList) {
    let state = window.global::<ArtistsState>();
    state.set_is_loading(false);
    state.set_is_complete(list.is_complete());
}

struct PageAnswer {
    generation: u64,
    start: usize,
    result: Result<Vec<Artist>, ApiError>,
}

fn apply_page_result(window: &AppWindow, answer: PageAnswer) {
    let Some(list) = shared_list(window) else {
        return;
    };
    let PageAnswer {
        generation,
        start,
        result,
    } = answer;
    if !list.is_current(generation) {
        return;
    }
    match result {
        Ok(artists) => {
            let items = artists.into_iter().map(ArtistItem::from).collect();
            list.apply_page(generation, start, items);
            finish_loading(window, &list);
        }
        Err(error) => {
            list.page_failed(generation);
            finish_loading(window, &list);
            show_status(window, &tr!("Could not load artists: {}", error));
        }
    }
}

pub(super) fn page_slot(query: &ArtistListQuery, start: usize) -> Option<ViewSlot> {
    let view = match query.is_favorite {
        Some(true) => View::Favorites,
        _ => View::Artists,
    };
    query
        .search
        .is_none()
        .then(|| view.slot(format!("artists/{query:?}/{start}")))
}

struct PageRequest {
    server: Arc<dyn MusicServer>,
    generation: u64,
    query: ArtistListQuery,
    start: usize,
}

fn load_page(weak_window: Weak<AppWindow>, context: &Context, request: PageRequest) {
    let PageRequest {
        server,
        generation,
        query,
        start,
    } = request;
    let views = context.views(&server);
    let slot = page_slot(&query, start);
    let fetch = async move {
        let paging = Paging {
            start_index: start,
            limit: PAGE_SIZE,
        };
        let page = server.artist_list(&query, paging).await?;
        Ok::<_, ApiError>(page.items)
    };
    views.load(slot, fetch, move |result| {
        let answer = PageAnswer {
            generation,
            start,
            result,
        };
        on_ui(&weak_window, move |window| {
            apply_page_result(window, answer);
        });
    });
}

struct CoverSlot {
    weak_window: Weak<AppWindow>,
    generation: u64,
    index: usize,
}

async fn fetch_cover(source: CoverSource, covers: Arc<dyn CoverProvider>, slot: CoverSlot) {
    let CoverSlot {
        weak_window,
        generation,
        index,
    } = slot;
    let cover = source.fetch(covers.as_ref(), COVER_SIZE).await;
    on_ui(&weak_window, move |window| {
        let image = cover.and_then(|cover| to_image(&cover));
        with_list(window, |list| list.apply_cover(generation, index, image));
    });
}

pub fn spawn_request(weak_window: Weak<AppWindow>, context: &Context, request: Request) {
    let Some(server) = context.session.get() else {
        return;
    };
    match request {
        Request::Page {
            generation,
            query,
            start,
        } => load_page(
            weak_window,
            context,
            PageRequest {
                server,
                generation,
                query,
                start,
            },
        ),
        Request::Cover {
            generation,
            index,
            cover_art_id,
        } => {
            let source = CoverSource::Server {
                server,
                cover_id: cover_art_id,
            };
            context.runtime.spawn(fetch_cover(
                source,
                context.view_covers(),
                CoverSlot {
                    weak_window,
                    generation,
                    index,
                },
            ));
        }
    }
}

pub fn reload(window: &AppWindow, search: &str) {
    let state = window.global::<ArtistsState>();
    let query = build_query(&ListControls {
        sort_index: state.get_sort_index(),
        is_descending: state.get_is_descending(),
        search,
        is_favorites_only: state.get_is_favorites_only(),
    });
    state.set_is_loading(true);
    state.set_is_complete(false);
    with_list(window, |list| list.reset(query));
}
