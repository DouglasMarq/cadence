use cadence_api::ApiError;
use cadence_core::model::AlbumDetail;
use cadence_storage::cover_cache::DecodedImage;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::super::album_grid::credited_artists;
use super::super::images::{CoverSource, tint_of, to_image};
use super::super::{AlbumDetailArtist, AlbumDetailState, AppWindow, Context, on_ui, show_status};
use super::info::album_info;
use super::tracks::show_tracks;

const COVER_SIZE: u32 = 440;

fn is_shown(window: &AppWindow, album_id: &SharedString) -> bool {
    window.global::<AlbumDetailState>().get_album_id() == *album_id
}

pub(super) fn show(window: &AppWindow, detail: AlbumDetail) {
    let state = window.global::<AlbumDetailState>();
    let album = &detail.album;
    let artists: Vec<AlbumDetailArtist> = credited_artists(album)
        .iter()
        .map(|artist| AlbumDetailArtist {
            id: artist.id.as_str().into(),
            name: artist.name.as_str().into(),
        })
        .collect();
    state.set_name(album.name.as_str().into());
    state.set_artists(ModelRc::new(VecModel::from(artists)));
    state.set_info(album_info(&detail).into());
    state.set_is_favorite(album.is_favorite);
    state.set_rating(i32::from(album.user_rating.unwrap_or(0)));
    show_tracks(window, detail.songs);
    state.set_is_loading(false);
}

fn show_error(window: &AppWindow, error: &ApiError) {
    let state = window.global::<AlbumDetailState>();
    state.set_is_loading(false);
    state.set_error(tr!("Album not found").into());
    show_status(window, &tr!("Could not load the album: {}", error));
}

fn clear(window: &AppWindow, album_id: &SharedString) {
    let state = window.global::<AlbumDetailState>();
    state.set_album_id(album_id.clone());
    state.set_is_loading(true);
    state.set_error(SharedString::default());
    state.set_name(SharedString::default());
    state.set_artists(ModelRc::default());
    state.set_info(SharedString::default());
    state.set_has_cover(false);
    state.set_cover(slint::Image::default());
    state.set_has_tint(false);
    state.set_is_favorite(false);
    state.set_rating(0);
    show_tracks(window, Vec::new());
}

fn show_cover(window: &AppWindow, album_id: &SharedString, cover: &DecodedImage) {
    if !is_shown(window, album_id) {
        return;
    }
    let Some(image) = to_image(cover) else {
        return;
    };
    let state = window.global::<AlbumDetailState>();
    state.set_cover(image);
    state.set_has_cover(true);
    if let Some(tint) = tint_of(cover) {
        state.set_tint(tint);
        state.set_has_tint(true);
    }
}

pub fn load(window: &AppWindow, context: &Context, album_id: SharedString) {
    clear(window, &album_id);
    let Some(server) = context.session.get() else {
        return;
    };
    let weak_window = window.as_weak();
    let covers = context.covers.clone();
    context.runtime.spawn(async move {
        let result = server.album(&album_id).await;
        let cover_id = result
            .as_ref()
            .ok()
            .and_then(|detail| detail.album.cover_art_id.clone());
        let shown_id = album_id.clone();
        on_ui(&weak_window, move |window| {
            if !is_shown(window, &shown_id) {
                return;
            }
            match result {
                Ok(detail) => show(window, detail),
                Err(error) => show_error(window, &error),
            }
        });
        let Some(cover_id) = cover_id else {
            return;
        };
        let source = CoverSource::Server { server, cover_id };
        let Some(cover) = source.fetch(covers.as_ref(), COVER_SIZE).await else {
            return;
        };
        on_ui(&weak_window, move |window| {
            show_cover(window, &album_id, &cover);
        });
    });
}
