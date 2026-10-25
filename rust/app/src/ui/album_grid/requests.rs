use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{AlbumListQuery, Page};
use slint::{Model, ModelRc, Weak};

use super::super::images::{CoverSource, to_image};
use super::super::page_cache::page_paging;
use super::super::{AlbumGridRow, AppWindow, Context, on_ui, show_status};
use super::card::CardData;
use super::model::{AlbumGridModel, GridRequest, PAGE_SIZE};
use crate::covers::CoverProvider;

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

struct GridFetch {
    server: Arc<dyn MusicServer>,
    target: GridTarget,
}

impl GridFetch {
    async fn count(self, generation: u64, query: AlbumListQuery) {
        let result = self.server.album_count(&query).await;
        let GridTarget {
            window,
            locate,
            on_count,
        } = self.target;
        on_ui(&window, move |window| {
            let total = result.as_ref().map_or(0, |total| *total);
            let is_current = with_model(window, locate, |model| model.is_current(generation));
            if is_current != Some(true) {
                return;
            }
            with_model(window, locate, |model| model.apply_count(generation, total));
            on_count(window, result.map_err(Into::into));
        });
    }

    async fn page(self, generation: u64, query: AlbumListQuery, page: usize) {
        let paging = page_paging(page, PAGE_SIZE);
        let result = self
            .server
            .album_list(&query, paging)
            .await
            .map(|albums| Page {
                items: albums.items.iter().map(CardData::from_album).collect(),
                start_index: albums.start_index,
                total_count: albums.total_count,
            });
        let GridTarget {
            window,
            locate,
            on_count,
        } = self.target;
        on_ui(&window, move |window| match result {
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

    async fn cover(self, covers: Arc<dyn CoverProvider>, cover_art_id: String) {
        let source = CoverSource::Server {
            server: self.server,
            cover_id: cover_art_id.clone(),
        };
        let cover = source.fetch(covers.as_ref(), COVER_SIZE).await;
        let locate = self.target.locate;
        on_ui(&self.target.window, move |window| {
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
    let covers = context.covers.clone();
    let fetch = GridFetch { server, target };
    context.runtime.spawn(async move {
        match request {
            GridRequest::Count { generation, query } => fetch.count(generation, query).await,
            GridRequest::Page {
                generation,
                query,
                page,
            } => fetch.page(generation, query, page).await,
            GridRequest::Cover { cover_art_id } => fetch.cover(covers, cover_art_id).await,
        }
    });
}
