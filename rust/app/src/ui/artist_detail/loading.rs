use std::sync::Arc;

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::ArtistDetail;
use cadence_storage::cover_cache::DecodedImage;
use slint::{ComponentHandle, Image};

use super::super::images::{CoverSource, to_image};
use super::super::{AppWindow, ArtistDetailState, Context, on_ui};
use super::current::{begin, is_current, update_current};
use super::view::{clear, set_album_image, show};
use crate::covers::CoverProvider;

const IMAGE_SIZE: u32 = 360;
const ALBUM_COVER_SIZE: u32 = 200;

async fn fetch_artist_image(
    server: Arc<dyn MusicServer>,
    covers: &dyn CoverProvider,
    detail: &Result<ArtistDetail, ApiError>,
) -> Option<Arc<DecodedImage>> {
    let cover_id = detail.as_ref().ok()?.artist.cover_art_id.clone()?;
    let source = CoverSource::Server { server, cover_id };
    source.fetch(covers, IMAGE_SIZE).await
}

fn show_result(
    window: &AppWindow,
    detail: Result<ArtistDetail, ApiError>,
    image: Option<Arc<DecodedImage>>,
) {
    match detail {
        Ok(detail) => show(window, detail, image.as_deref()),
        Err(error) => {
            update_current(|current| current.id.clear());
            let state = window.global::<ArtistDetailState>();
            state.set_is_loading(false);
            state.set_error(tr!("Could not load the artist: {}", error).into());
        }
    }
}

pub fn load(window: &AppWindow, context: &Context, artist_id: &str) {
    let Some(generation) = begin(artist_id) else {
        return;
    };
    clear(window);
    window.global::<ArtistDetailState>().set_is_loading(true);
    let Some(server) = context.session.get() else {
        return;
    };
    let weak_window = window.as_weak();
    let covers = context.covers.clone();
    let artist_id = artist_id.to_owned();
    context.runtime.spawn(async move {
        let detail = server.artist(&artist_id).await;
        let image = fetch_artist_image(server, covers.as_ref(), &detail).await;
        on_ui(&weak_window, move |window| {
            if is_current(generation) {
                show_result(window, detail, image);
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
        let cover_id = current.album_covers.get(index).cloned().flatten()?;
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
    let covers = context.covers.clone();
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
