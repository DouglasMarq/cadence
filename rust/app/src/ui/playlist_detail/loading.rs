use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{Playlist, Song};
use cadence_storage::cover_cache::DecodedImage;
use slint::ComponentHandle;

use super::binding::with_model;
use super::deps::DetailDeps;
use super::detail::{SharedDetail, lock};
use super::pages::paging_for_page;
use super::rows::{HeaderSummary, header_info};
use crate::ui::images::{CoverClaim, CoverSource, tint_of, to_image};
use crate::ui::{
    AppState, AppWindow, Context, PlaylistDetailState, Route, clamp_i32, on_ui, show_status,
};
use crate::views::{View, ViewSlot};

const COVER_SIZE: u32 = 300;
const METADATA_ITEM: &str = "metadata";
const FIRST_PAGE_ITEM: &str = "first-page";

type FirstPage = (Option<usize>, Vec<Song>);

fn playlist_slot(playlist_id: &str, item: &str) -> ViewSlot {
    View::Playlist(playlist_id.to_owned()).slot(item)
}

#[derive(Clone)]
struct ShownDetail {
    shared: SharedDetail,
    generation: u64,
}

#[derive(Clone)]
struct OpenedPlaylist {
    window: slint::Weak<AppWindow>,
    shown: ShownDetail,
    context: Context,
    server: Arc<dyn MusicServer>,
    playlist_id: String,
}

fn reset_view(window: &AppWindow, generation: u64, playlist_id: &str) {
    let state = window.global::<PlaylistDetailState>();
    state.set_name("".into());
    state.set_comment("".into());
    state.set_info("".into());
    state.set_has_cover(false);
    state.set_cover(slint::Image::default());
    state.set_has_tint(false);
    state.set_selected_count(0);
    state.set_is_busy(false);
    with_model(window, |model| model.reset(generation, playlist_id));
}

pub fn load(window: &AppWindow, deps: &DetailDeps, playlist_id: &str) {
    let DetailDeps {
        context, shared, ..
    } = deps;
    context.session.open_view();
    let generation = lock(shared).restart(playlist_id);
    reset_view(window, generation, playlist_id);
    let state = window.global::<PlaylistDetailState>();
    let Some(server) = context.session.get().filter(|_| !playlist_id.is_empty()) else {
        state.set_is_loading(false);
        return;
    };
    state.set_is_loading(true);
    let opened = OpenedPlaylist {
        window: window.as_weak(),
        shown: ShownDetail {
            shared: Arc::clone(shared),
            generation,
        },
        context: context.clone(),
        server,
        playlist_id: playlist_id.to_owned(),
    };
    opened.clone().load_metadata();
    opened.load_first_page();
}

impl OpenedPlaylist {
    fn load_metadata(self) {
        let (server, playlist_id) = (Arc::clone(&self.server), self.playlist_id.clone());
        let slot = playlist_slot(&playlist_id, METADATA_ITEM);
        let metadata = async move { server.playlist(&playlist_id).await };
        let claim = CoverClaim::default();
        let views = self.context.views(&self.server);
        views.load(Some(slot), metadata, move |result| {
            self.deliver_metadata(&claim, result);
        });
    }

    fn deliver_metadata(&self, claim: &CoverClaim, result: cadence_api::Result<Playlist>) {
        let metadata = result
            .inspect_err(|error| tracing::debug!("playlist metadata: {error}"))
            .ok();
        let cover_id = metadata
            .as_ref()
            .and_then(|playlist| playlist.cover_art_id.as_deref());
        if let Some(cover_id) = claim.claim(cover_id) {
            self.load_cover(cover_id);
        }
        let shown = self.shown.clone();
        on_ui(&self.window, move |window| {
            shown.apply_metadata(window, metadata);
        });
    }

    fn load_first_page(self) {
        let (server, playlist_id) = (Arc::clone(&self.server), self.playlist_id.clone());
        let slot = playlist_slot(&playlist_id, FIRST_PAGE_ITEM);
        let first_page = async move {
            let page = server
                .playlist_song_page(&playlist_id, paging_for_page(0))
                .await?;
            cadence_api::Result::Ok((page.total_count, page.items))
        };
        let views = self.context.views(&self.server);
        views.load(Some(slot), first_page, move |result| {
            let shown = self.shown.clone();
            on_ui(&self.window, move |window| {
                shown.apply_first_page(window, result);
            });
        });
    }

    fn load_cover(&self, cover_id: String) {
        let covers = self.context.view_covers();
        let (window, shown) = (self.window.clone(), self.shown.clone());
        let server = Arc::clone(&self.server);
        self.context.runtime.spawn(async move {
            let source = CoverSource::Server { server, cover_id };
            let Some(cover) = source.fetch(covers.as_ref(), COVER_SIZE).await else {
                return;
            };
            on_ui(&window, move |window| {
                if lock(&shown.shared).is_current(shown.generation) {
                    show_cover(window, &cover);
                }
            });
        });
    }
}

fn show_cover(window: &AppWindow, cover: &DecodedImage) {
    let Some(image) = to_image(cover) else {
        return;
    };
    let state = window.global::<PlaylistDetailState>();
    state.set_cover(image);
    state.set_has_cover(true);
    if let Some(tint) = tint_of(cover) {
        state.set_tint(tint);
        state.set_has_tint(true);
    }
}

impl ShownDetail {
    fn apply_metadata(&self, window: &AppWindow, metadata: Option<Playlist>) {
        let mut detail = lock(&self.shared);
        if !detail.is_current(self.generation) {
            return;
        }
        detail.adopt_metadata(metadata);
        let awaited_total = detail.total_without_a_page_report();
        let name = detail.name.clone();
        let shown = detail.metadata.clone();
        let is_settled = detail.is_settled();
        drop(detail);
        window
            .global::<PlaylistDetailState>()
            .set_is_loading(!is_settled);
        if let Some(total) = awaited_total {
            with_model(window, |model| model.grow_to(self.generation, total));
        }
        show_header(window, &name, shown.as_ref());
        show_comment(window, shown.as_ref());
    }

    fn apply_first_page(&self, window: &AppWindow, result: cadence_api::Result<FirstPage>) {
        let mut detail = lock(&self.shared);
        if !detail.is_current(self.generation) {
            return;
        }
        let state = window.global::<PlaylistDetailState>();
        let (reported_total, songs) = match result {
            Ok(first_page) => first_page,
            Err(error) => {
                detail.first_page_failed();
                state.set_is_loading(false);
                drop(detail);
                show_status(window, &tr!("Could not load the playlist: {}", error));
                return;
            }
        };
        let total = detail.adopt_first_page(reported_total, songs.len());
        let name = detail.name.clone();
        let shown = detail.metadata.clone();
        state.set_is_loading(!detail.is_settled());
        drop(detail);
        with_model(window, |model| {
            model.apply_first_page(self.generation, total, songs);
        });
        if let Some(selected) = with_model(window, |model| model.selected_count()) {
            state.set_selected_count(clamp_i32(selected as u64));
        }
        show_header(window, &name, shown.as_ref());
    }
}

fn show_header(window: &AppWindow, name: &str, metadata: Option<&Playlist>) {
    let loaded_total = with_model(window, |model| model.total()).unwrap_or_default();
    let state = window.global::<PlaylistDetailState>();
    state.set_name(name.into());
    state.set_info(header_info(&HeaderSummary::new(metadata, loaded_total)).into());
}

fn show_comment(window: &AppWindow, metadata: Option<&Playlist>) {
    let comment = metadata
        .and_then(|playlist| playlist.comment.as_deref())
        .unwrap_or_default()
        .trim();
    window
        .global::<PlaylistDetailState>()
        .set_comment(comment.into());
}

pub fn refresh_if_showing(window: &AppWindow, playlist_id: &str) {
    let app = window.global::<AppState>();
    if app.get_route() == Route::PlaylistDetail && app.get_detail_id() == playlist_id {
        window
            .global::<PlaylistDetailState>()
            .invoke_load(playlist_id.into());
    }
}
