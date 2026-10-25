use slint::{ComponentHandle, ModelRc};

use super::binding::{Binding, with_grid};
use super::card::{CardPage, CardSource};
use super::card_grid::CardGrid;
use super::grid_state::{GRID_PAGE, GridRequest};
use crate::ui::images::{CoverSource, to_image};
use crate::ui::page_cache::page_paging;
use crate::ui::{AppWindow, CardRow, Context, on_ui, show_status};

const GRID_COVER_SIZE: u32 = 200;

pub fn card_grid(
    window: &AppWindow,
    context: &Context,
    binding: Binding<CardRow>,
) -> ModelRc<CardRow> {
    let loader = GridLoader {
        window: window.as_weak(),
        context: context.clone(),
        binding,
    };
    ModelRc::new(CardGrid::new(move |request| loader.spawn(request)))
}

struct GridLoader {
    window: slint::Weak<AppWindow>,
    context: Context,
    binding: Binding<CardRow>,
}

struct FetchedCards {
    generation: u64,
    page: usize,
    result: cadence_api::Result<CardPage>,
}

impl GridLoader {
    fn spawn(&self, request: GridRequest) {
        match request {
            GridRequest::Page {
                generation,
                source,
                page,
            } => self.spawn_page(generation, source, page),
            GridRequest::Cover { cover_id } => self.spawn_cover(cover_id),
        }
    }

    fn spawn_page(&self, generation: u64, source: CardSource, page: usize) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let (window, binding) = (self.window.clone(), self.binding);
        self.context.runtime.spawn(async move {
            let result = source
                .fetch(server.as_ref(), page_paging(page, GRID_PAGE))
                .await;
            let fetched = FetchedCards {
                generation,
                page,
                result,
            };
            on_ui(&window, move |window| {
                apply_grid_page(window, binding, fetched);
            });
        });
    }

    fn spawn_cover(&self, cover_id: String) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let (window, binding) = (self.window.clone(), self.binding);
        let covers = self.context.covers.clone();
        self.context.runtime.spawn(async move {
            let source = CoverSource::Server {
                server,
                cover_id: cover_id.clone(),
            };
            let cover = source.fetch(covers.as_ref(), GRID_COVER_SIZE).await;
            on_ui(&window, move |window| {
                let image = cover.and_then(|cover| to_image(&cover));
                with_grid(window, binding, |grid| grid.apply_cover(&cover_id, image));
            });
        });
    }
}

fn apply_grid_page(window: &AppWindow, binding: Binding<CardRow>, fetched: FetchedCards) {
    let FetchedCards {
        generation,
        page,
        result,
    } = fetched;
    match result {
        Ok(listed) => {
            let is_current = with_grid(window, binding, |grid| grid.is_current(generation));
            if is_current != Some(true) {
                return;
            }
            with_grid(window, binding, |grid| {
                grid.apply_page(generation, page, listed)
            });
            (binding.loading)(window, false);
        }
        Err(error) => {
            let is_waiting = with_grid(window, binding, |grid| {
                grid.is_waiting_for(generation, page)
            });
            if is_waiting != Some(true) {
                return;
            }
            with_grid(window, binding, |grid| grid.page_failed(generation, page));
            (binding.loading)(window, false);
            show_status(window, &tr!("Could not load the list: {}", error));
        }
    }
}
