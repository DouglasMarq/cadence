use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{Album, Page};
use slint::{Model, ModelRc, Weak};

use super::super::images::{CoverSource, to_image};
use super::super::page_cache::page_paging;
use super::super::{AlbumGridRow, AppWindow, Context, on_ui, show_status};
use super::card::CardData;
use super::model::{AlbumGridModel, GridRequest, PAGE_SIZE};
use crate::covers::CoverProvider;
use crate::views::{count_slot, page_slot};

pub const COVER_SIZE: u32 = 256;

pub type Locate = fn(&AppWindow) -> ModelRc<AlbumGridRow>;

pub type OnCount = fn(&AppWindow, anyhow::Result<usize>);

pub fn with_model<Output>(
    window: &AppWindow,
    locate: Locate,
    action: impl FnOnce(&AlbumGridModel) -> Output,
) -> Option<Output> {
    let rows = locate(window);
    rows.as_any().downcast_ref::<AlbumGridModel>().map(action)
}

pub struct GridTarget {
    pub window: Weak<AppWindow>,
    pub locate: Locate,
    pub on_count: OnCount,
}

struct AlbumPage {
    generation: u64,
    page: usize,
    result: cadence_api::Result<Page<Album>>,
}

impl GridTarget {
    fn show_count(&self, generation: u64, result: cadence_api::Result<usize>) {
        let (locate, on_count) = (self.locate, self.on_count);
        on_ui(&self.window, move |window| {
            let total = result.as_ref().map_or(0, |total| *total);
            let is_current = with_model(window, locate, |model| model.is_current(generation));
            if is_current != Some(true) {
                return;
            }
            with_model(window, locate, |model| model.apply_count(generation, total));
            on_count(window, result.map_err(Into::into));
        });
    }

    fn show_page(&self, answer: AlbumPage) {
        let AlbumPage {
            generation,
            page,
            result,
        } = answer;
        let (locate, on_count) = (self.locate, self.on_count);
        let result = result.map(|albums| Page {
            items: albums.items.iter().map(CardData::from_album).collect(),
            start_index: albums.start_index,
            total_count: albums.total_count,
        });
        on_ui(&self.window, move |window| match result {
            Ok(listed) => {
                let changed = with_model(window, locate, |model| {
                    model.reported_total_change(generation, listed.total_count)
                });
                with_model(window, locate, |model| {
                    model.apply_listed_page(generation, page, listed);
                });
                if let Some(Some(total)) = changed {
                    on_count(window, Ok(total));
                }
            }
            Err(error) => {
                with_model(window, locate, |model| model.page_failed(generation, page));
                show_status(window, &tr!("Could not load albums: {}", error));
            }
        });
    }

    async fn fetch_cover(
        self,
        server: Arc<dyn MusicServer>,
        covers: Arc<dyn CoverProvider>,
        cover_art_id: String,
    ) {
        let source = CoverSource::Server {
            server,
            cover_id: cover_art_id.clone(),
        };
        let cover = source.fetch(covers.as_ref(), COVER_SIZE).await;
        let locate = self.locate;
        on_ui(&self.window, move |window| {
            let image = cover.and_then(|cover| to_image(&cover));
            with_model(window, locate, |model| {
                model.apply_cover(&cover_art_id, image)
            });
        });
    }
}

pub fn spawn_request(target: GridTarget, context: &Context, request: GridRequest) {
    let Some(server) = context.session.get() else {
        return;
    };
    match request {
        GridRequest::Count { generation, query } => {
            let slot = count_slot(&query);
            let views = context.views(&server);
            let count = async move { server.album_count(&query).await };
            views.load(slot, count, move |result| {
                target.show_count(generation, result);
            });
        }
        GridRequest::Page {
            generation,
            query,
            page,
        } => {
            let slot = page_slot(&query, page);
            let views = context.views(&server);
            let paging = page_paging(page, PAGE_SIZE);
            let albums = async move { server.album_list(&query, paging).await };
            views.load(slot, albums, move |result| {
                target.show_page(AlbumPage {
                    generation,
                    page,
                    result,
                });
            });
        }
        GridRequest::Cover { cover_art_id } => {
            let covers = context.view_covers();
            context
                .runtime
                .spawn(target.fetch_cover(server, covers, cover_art_id));
        }
    }
}
