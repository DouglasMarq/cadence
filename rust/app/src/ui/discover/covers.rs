use std::sync::Arc;

use cadence_storage::cover_cache::DecodedImage;
use slint::{ComponentHandle, Image};

use super::cards::{CardPosition, cover_key};
use super::deps::DiscoverView;
use super::model::with_discover;
use crate::ui::images::to_image;
use crate::ui::{AppWindow, DiscoverState, on_ui};

const CARD_COVER_SIZE: u32 = 192;
const SONG_COVER_SIZE: u32 = 64;
const PAGE_COVER_SIZE: u32 = 300;

struct CoverFetch {
    url: String,
    size: u32,
}

fn fetch_cover(
    DiscoverView { window, deps }: DiscoverView,
    CoverFetch { url, size }: CoverFetch,
    done: impl FnOnce(&AppWindow, Option<Image>) + Send + 'static,
) {
    let covers = Arc::clone(&deps.context.covers);
    let weak_window = window.as_weak();
    deps.context.runtime.spawn(async move {
        let decoded: Option<Arc<DecodedImage>> = covers
            .cover_from_url(&cover_key(&url), &url, size)
            .await
            .inspect_err(|error| tracing::debug!("YouTube Music cover {url}: {error:#}"))
            .ok();
        on_ui(&weak_window, move |window| {
            done(window, decoded.and_then(|cover| to_image(&cover)));
        });
    });
}

pub fn load_card_cover(view: DiscoverView, position: CardPosition) {
    let claimed = with_discover(view.window, |model| {
        model.request_card_cover(position.shelf, position.card)
    })
    .flatten();
    let Some((request, card_key)) = claimed else {
        return;
    };
    let generation = request.generation;
    let fetch = CoverFetch {
        url: request.url,
        size: CARD_COVER_SIZE,
    };
    fetch_cover(view, fetch, move |window, cover| {
        with_discover(window, |model| {
            model.set_card_cover(generation, card_key, cover);
        });
    });
}

pub fn load_song_cover(view: DiscoverView, index: usize) {
    let Some(request) =
        with_discover(view.window, |model| model.request_song_cover(index)).flatten()
    else {
        return;
    };
    let generation = request.generation;
    let fetch = CoverFetch {
        url: request.url,
        size: SONG_COVER_SIZE,
    };
    fetch_cover(view, fetch, move |window, cover| {
        with_discover(window, |model| {
            model.set_song_cover(generation, index, cover)
        });
    });
}

pub fn load_page_cover(view: DiscoverView, generation: u64, url: String) {
    let fetch = CoverFetch {
        url,
        size: PAGE_COVER_SIZE,
    };
    fetch_cover(view, fetch, move |window, cover| {
        let is_current =
            with_discover(window, |model| model.page.borrow().generation == generation);
        let (Some(true), Some(cover)) = (is_current, cover) else {
            return;
        };
        let state = window.global::<DiscoverState>();
        state.set_page_cover(cover);
        state.set_has_page_cover(true);
    });
}

pub fn card_visibility_changed(window: &AppWindow, position: CardPosition, is_visible: bool) {
    with_discover(window, |model| {
        model.set_card_visible(position.shelf, position.card, is_visible)
    });
}

pub fn song_rows_shown(window: &AppWindow, first_row: usize, row_count: usize) {
    with_discover(window, |model| {
        model.show_song_rows(first_row..first_row.saturating_add(row_count));
    });
}
