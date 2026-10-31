use std::sync::Arc;

use cadence_storage::cover_cache::DecodedImage;
use slint::{ComponentHandle, Image, Model};

use super::super::images::{CoverSource, tint_of, to_image};
use super::super::{AppWindow, ArtistDetailState, Context, on_ui};
use super::current::{is_current, update_current};

const IMAGE_SIZE: u32 = 360;
const ALBUM_COVER_SIZE: u32 = 200;

fn set_album_image(window: &AppWindow, index: usize, image: Option<Image>) {
    let model = window.global::<ArtistDetailState>().get_albums();
    if let Some(mut card) = model.row_data(index) {
        card.has_image = image.is_some();
        card.image = image.unwrap_or_default();
        model.set_row_data(index, card);
    }
}

fn show_artist_image(window: &AppWindow, cover: &DecodedImage) {
    let state = window.global::<ArtistDetailState>();
    if let Some(image) = to_image(cover) {
        state.set_image(image);
        state.set_has_image(true);
    }
    if let Some(tint) = tint_of(cover) {
        state.set_tint(tint);
        state.set_has_tint(true);
    }
}

fn claim_artist_image(cover_id: &str) -> Option<u64> {
    update_current(|current| {
        if current.image_cover_id.as_deref() == Some(cover_id) {
            return None;
        }
        current.image_cover_id = Some(cover_id.to_owned());
        Some(current.generation)
    })
}

pub fn load_artist_image(window: &AppWindow, context: &Context, cover_id: &str) {
    let claim = claim_artist_image(cover_id);
    let (Some(generation), Some(server)) = (claim, context.session.get()) else {
        return;
    };
    let weak_window = window.as_weak();
    let covers = context.view_covers();
    let cover_id = cover_id.to_owned();
    context.runtime.spawn(async move {
        let source = CoverSource::Server { server, cover_id };
        let cover: Option<Arc<DecodedImage>> = source.fetch(covers.as_ref(), IMAGE_SIZE).await;
        on_ui(&weak_window, move |window| {
            if let (true, Some(cover)) = (is_current(generation), cover) {
                show_artist_image(window, &cover);
            }
        });
    });
}

#[derive(Clone, Copy)]
struct AlbumCoverSlot {
    generation: u64,
    index: usize,
}

fn claim_album_cover(index: usize) -> Option<(u64, String)> {
    update_current(|current| {
        let cover_id = current.album_cover_id(index)?;
        current
            .covers
            .begin(&index)
            .then_some((current.generation, cover_id))
    })
}

pub fn load_album_cover(window: &AppWindow, context: &Context, index: usize) {
    let request = claim_album_cover(index);
    let (Some((generation, cover_id)), Some(server)) = (request, context.session.get()) else {
        return;
    };
    let slot = AlbumCoverSlot { generation, index };
    let weak_window = window.as_weak();
    let covers = context.view_covers();
    context.runtime.spawn(async move {
        let source = CoverSource::Server { server, cover_id };
        let cover = source.fetch(covers.as_ref(), ALBUM_COVER_SIZE).await;
        on_ui(&weak_window, move |window| {
            let image = cover.and_then(|cover| to_image(&cover));
            apply_album_cover(window, slot, image);
        });
    });
}

fn apply_album_cover(window: &AppWindow, slot: AlbumCoverSlot, image: Option<Image>) {
    let AlbumCoverSlot { generation, index } = slot;
    let evicted = update_current(|current| {
        if current.generation != generation {
            return None;
        }
        match &image {
            Some(image) => Some(current.covers.insert(index, image.clone())),
            None => {
                current.covers.retry_later(&index);
                None
            }
        }
    });
    let Some(evicted) = evicted else {
        return;
    };
    if let Some(evicted_index) = evicted {
        set_album_image(window, evicted_index, None);
    }
    set_album_image(window, index, image);
}
