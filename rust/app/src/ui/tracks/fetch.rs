use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{Page, Song};
use slint::ComponentHandle;

use super::model::{Request, paging_for_page};
use super::with_model;
use crate::ui::{AppWindow, Context, TracksState, clamp_i32, on_ui, show_status};

pub struct FetchedCount {
    pub generation: u64,
    pub result: cadence_api::Result<usize>,
}

pub struct FetchedPage {
    pub generation: u64,
    pub page: usize,
    pub result: cadence_api::Result<Page<Song>>,
}

pub enum Fetched {
    Count(FetchedCount),
    Page(FetchedPage),
}

pub fn spawn_fetch(
    context: &Context,
    request: Request,
    done: impl FnOnce(Fetched) + Send + 'static,
) {
    let Some(server) = context.session.get() else {
        return;
    };
    context.runtime.spawn(async move {
        done(fetch(server, request).await);
    });
}

async fn fetch(server: Arc<dyn MusicServer>, request: Request) -> Fetched {
    match request {
        Request::Count { generation, query } => Fetched::Count(FetchedCount {
            generation,
            result: server.song_count(&query).await,
        }),
        Request::Page {
            generation,
            query,
            page,
        } => Fetched::Page(FetchedPage {
            generation,
            page,
            result: server.song_list(&query, paging_for_page(page)).await,
        }),
    }
}

pub(super) fn spawn_request(window: slint::Weak<AppWindow>, context: &Context, request: Request) {
    spawn_fetch(context, request, move |fetched| {
        on_ui(&window, move |window| match fetched {
            Fetched::Count(count) => show_count(window, count),
            Fetched::Page(page) => show_page(window, page),
        });
    });
}

fn show_count(window: &AppWindow, count: FetchedCount) {
    let total = count.result.as_ref().map_or(0, |total| *total);
    let is_current = with_model(window, |model| model.is_current(count.generation));
    if is_current != Some(true) {
        return;
    }
    with_model(window, |model| model.apply_count(count.generation, total));
    let state = window.global::<TracksState>();
    state.set_total(clamp_i32(total as u64));
    state.set_is_loading(false);
    if let Err(error) = count.result {
        show_status(window, &tr!("Could not load tracks: {}", error));
    }
}

fn show_page(window: &AppWindow, fetched: FetchedPage) {
    let FetchedPage {
        generation,
        page,
        result,
    } = fetched;
    match result {
        Ok(songs) => {
            let changed = with_model(window, |model| {
                model.reported_total_change(generation, songs.total_count)
            });
            with_model(window, |model| {
                model.apply_listed_page(generation, page, songs);
            });
            if let Some(Some(total)) = changed {
                let state = window.global::<TracksState>();
                state.set_total(clamp_i32(total as u64));
                state.set_selected_count(0);
            }
        }
        Err(error) => {
            with_model(window, |model| model.page_failed(generation, page));
            show_status(window, &tr!("Could not load tracks: {}", error));
        }
    }
}
