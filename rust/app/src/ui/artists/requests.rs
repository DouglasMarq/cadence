use std::rc::Rc;
use std::sync::Arc;

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::{ArtistListQuery, Paging};
use slint::{ComponentHandle, Model, Weak};

use super::super::images::{CoverSource, to_image};
use super::super::{AppWindow, ArtistsState, Context, on_ui, show_status};
use super::item::ArtistItem;
use super::list::{ArtistList, PAGE_SIZE, Request};
use super::models::ListModel;
use super::query::build_query;
use crate::covers::CoverProvider;
use crate::ui::ListControls;

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
    result: Result<Vec<ArtistItem>, ApiError>,
}

struct ArtistFetch {
    server: Arc<dyn MusicServer>,
    covers: Arc<dyn CoverProvider>,
    weak_window: Weak<AppWindow>,
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
    match result {
        Ok(items) => {
            if !list.is_next_page(generation, start) {
                return;
            }
            list.apply_page(generation, start, items);
            finish_loading(window, &list);
        }
        Err(error) => {
            if !list.is_current(generation) {
                return;
            }
            list.page_failed(generation);
            finish_loading(window, &list);
            show_status(window, &tr!("Could not load artists: {}", error));
        }
    }
}

impl ArtistFetch {
    async fn page(self, generation: u64, query: ArtistListQuery, start: usize) {
        let paging = Paging {
            start_index: start,
            limit: PAGE_SIZE,
        };
        let result = self
            .server
            .artist_list(&query, paging)
            .await
            .map(|page| page.items.into_iter().map(ArtistItem::from).collect());
        let answer = PageAnswer {
            generation,
            start,
            result,
        };
        on_ui(&self.weak_window, move |window| {
            apply_page_result(window, answer);
        });
    }

    async fn cover(self, generation: u64, index: usize, cover_art_id: String) {
        let source = CoverSource::Server {
            server: self.server,
            cover_id: cover_art_id,
        };
        let cover = source.fetch(self.covers.as_ref(), COVER_SIZE).await;
        on_ui(&self.weak_window, move |window| {
            let image = cover.and_then(|cover| to_image(&cover));
            with_list(window, |list| list.apply_cover(generation, index, image));
        });
    }
}

pub fn spawn_request(weak_window: Weak<AppWindow>, context: &Context, request: Request) {
    let Some(server) = context.session.get() else {
        return;
    };
    let fetch = ArtistFetch {
        server,
        covers: context.covers.clone(),
        weak_window,
    };
    context.runtime.spawn(async move {
        match request {
            Request::Page {
                generation,
                query,
                start,
            } => fetch.page(generation, query, start).await,
            Request::Cover {
                generation,
                index,
                cover_art_id,
            } => fetch.cover(generation, index, cover_art_id).await,
        }
    });
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
