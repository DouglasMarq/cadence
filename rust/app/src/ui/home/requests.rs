use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cadence_core::model::{Album, Paging};
use slint::ComponentHandle;

use super::super::images::{CoverSource, to_image};
use super::super::{AppWindow, Context, on_ui, show_status};
use super::model::with_home_model;
use super::shelf::SHELF_SIZE;
use crate::controller::Command;
use crate::views::shelf_slot;

const COVER_SIZE: u32 = 192;

pub fn load_shelf(window: &AppWindow, context: &Context, shelf: usize) {
    let Some(server) = context.session.get() else {
        return;
    };
    let Some(Some((generation, query))) = with_home_model(window, |model| model.begin_fetch(shelf))
    else {
        return;
    };
    let weak_window = window.as_weak();
    let views = context.views(&server);
    let slot = shelf_slot(&query);
    let paging = Paging {
        start_index: 0,
        limit: SHELF_SIZE,
    };
    let albums = async move {
        let page = server.album_list(&query, paging).await?;
        cadence_api::Result::Ok(page.items)
    };
    views.load(slot, albums, move |result| {
        on_ui(&weak_window, move |window| {
            show_shelf(window, (shelf, generation), result);
        });
    });
}

fn show_shelf(
    window: &AppWindow,
    (shelf, generation): (usize, u64),
    result: cadence_api::Result<Vec<Album>>,
) {
    match result {
        Ok(albums) => {
            with_home_model(window, |model| {
                model.apply_fetched(shelf, generation, &albums)
            });
        }
        Err(error) => {
            let is_current_fetch =
                with_home_model(window, |model| model.is_current_fetch(shelf, generation));
            if is_current_fetch != Some(true) {
                return;
            }
            with_home_model(window, |model| model.fetch_failed(shelf, generation));
            show_status(window, &tr!("Could not load albums: {}", error));
        }
    }
}

#[derive(Clone, Copy)]
pub struct CardPosition {
    pub shelf: usize,
    pub index: usize,
}

pub fn load_cover(window: &AppWindow, context: &Context, position: CardPosition) {
    let Some(server) = context.session.get() else {
        return;
    };
    let CardPosition { shelf, index } = position;
    let Some(Some((album_id, cover_id))) =
        with_home_model(window, |model| model.request_cover(shelf, index))
    else {
        return;
    };
    let covers = context.view_covers();
    let weak_window = window.as_weak();
    context.runtime.spawn(async move {
        let source = CoverSource::Server { server, cover_id };
        let cover = source.fetch(covers.as_ref(), COVER_SIZE).await;
        on_ui(&weak_window, move |window| {
            let image = cover.and_then(|cover| to_image(&cover));
            with_home_model(window, |model| model.set_cover(shelf, &album_id, image));
        });
    });
}

pub struct AlbumPlayer {
    pub context: Context,
    pub latest_request: Arc<AtomicU64>,
}

impl AlbumPlayer {
    pub fn play(&self, window: &AppWindow, album_id: String) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let request = self.latest_request.fetch_add(1, Ordering::Relaxed) + 1;
        let latest_request = Arc::clone(&self.latest_request);
        let context = self.context.clone();
        let weak_window = window.as_weak();
        context.runtime.clone().spawn(async move {
            match server.album(&album_id).await {
                Ok(detail) if latest_request.load(Ordering::Relaxed) == request => {
                    context.send(Command::PlaySongs {
                        songs: detail.songs,
                        song_id: None,
                    });
                }
                Ok(_) => {}
                Err(error) => on_ui(&weak_window, move |window| {
                    show_status(window, &tr!("Could not play album: {}", error));
                }),
            }
        });
    }
}
