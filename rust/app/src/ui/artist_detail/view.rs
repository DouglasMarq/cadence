use cadence_core::model::{Album, ArtistDetail, ArtistPart};
use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};

use super::super::rows::sync_rows;
use super::super::tracks::track_row;
use super::super::{AppWindow, ArtistAlbumCard, ArtistDetailState, Context, TrackRow};
use super::biography::plain_text;
use super::covers::{load_album_cover, load_artist_image};
use super::current::{read_current, update_current};
use super::info::{album_card, info_line};

pub fn clear(window: &AppWindow) {
    let state = window.global::<ArtistDetailState>();
    state.set_error("".into());
    state.set_name("".into());
    state.set_info("".into());
    state.set_image(Image::default());
    state.set_has_image(false);
    state.set_has_tint(false);
    state.set_is_favorite(false);
    state.set_biography("".into());
    state.set_is_bio_expanded(false);
    state.set_is_top_expanded(false);
    let (top_rows, album_cards) =
        read_current(|current| (current.top_rows.clone(), current.album_cards.clone()));
    state.set_top_songs(ModelRc::from(top_rows));
    state.set_albums(ModelRc::from(album_cards));
}

pub fn show_loading(window: &AppWindow, is_loading: bool) {
    let state = window.global::<ArtistDetailState>();
    state.set_is_loading(is_loading);
    state.set_is_loading_biography(is_loading);
    state.set_is_loading_top_songs(is_loading);
    state.set_is_loading_albums(is_loading);
}

fn album_cards_keeping_images(
    shown: &VecModel<ArtistAlbumCard>,
    albums: &[Album],
) -> Vec<ArtistAlbumCard> {
    albums
        .iter()
        .enumerate()
        .map(|(index, album)| {
            let mut card = album_card(album);
            if let Some(previous) = shown.row_data(index).filter(|row| row.id == card.id) {
                card.image = previous.image;
                card.has_image = previous.has_image;
            }
            card
        })
        .collect()
}

fn indexes_showing_another_album(
    shown: &VecModel<ArtistAlbumCard>,
    albums: &[Album],
) -> Vec<usize> {
    albums
        .iter()
        .enumerate()
        .filter(|(index, album)| {
            shown
                .row_data(*index)
                .is_some_and(|row| row.id != album.id.as_str())
        })
        .map(|(index, _album)| index)
        .collect()
}

fn show_albums(window: &AppWindow, context: &Context, albums: &[Album]) {
    let shown = read_current(|current| current.album_cards.clone());
    let replaced = indexes_showing_another_album(&shown, albums);
    let cards = album_cards_keeping_images(&shown, albums);
    sync_rows(&shown, cards);
    let requested_before: Vec<usize> = update_current(|current| {
        replaced
            .into_iter()
            .filter(|index| current.covers.forget(index))
            .collect()
    });
    for index in requested_before {
        load_album_cover(window, context, index);
    }
}

fn show_top_songs(detail: &ArtistDetail) {
    let rows: Vec<TrackRow> = detail
        .top_songs
        .iter()
        .enumerate()
        .map(|(position, song)| track_row(song, position))
        .collect();
    let shown = read_current(|current| current.top_rows.clone());
    sync_rows(&shown, rows);
}

pub fn render(window: &AppWindow, context: &Context) {
    let Some(detail) = read_current(|current| current.detail.clone()) else {
        return;
    };
    let state = window.global::<ArtistDetailState>();
    state.set_name(detail.artist.name.as_str().into());
    state.set_info(info_line(&detail).into());
    state.set_is_favorite(detail.artist.is_favorite);
    state.set_biography(plain_text(detail.artist.biography.as_deref().unwrap_or_default()).into());
    show_top_songs(&detail);
    show_albums(window, context, &detail.albums);
    if let Some(cover_id) = &detail.artist.cover_art_id {
        load_artist_image(window, context, cover_id);
    }
}

pub fn show_part(window: &AppWindow, context: &Context, part: ArtistPart) {
    let state = window.global::<ArtistDetailState>();
    match &part {
        ArtistPart::Profile(_) => {}
        ArtistPart::Albums(_) => state.set_is_loading_albums(false),
        ArtistPart::TopSongs(_) => state.set_is_loading_top_songs(false),
        ArtistPart::Biography(_) => state.set_is_loading_biography(false),
    }
    update_current(|current| part.merge_into(&mut current.detail));
    render(window, context);
}

pub fn show_detail(window: &AppWindow, context: &Context, detail: ArtistDetail) {
    show_loading(window, false);
    update_current(|current| current.detail = Some(detail));
    render(window, context);
}
