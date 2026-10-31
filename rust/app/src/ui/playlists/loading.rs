use cadence_core::model::Playlist;
use slint::{ComponentHandle, Model};

use super::catalog::fetch_all;
use super::model::PlaylistsModel;
use crate::ui::images::{CoverSource, to_image};
use crate::ui::{AppState, AppWindow, Context, PlaylistsState, Route, on_ui, show_status};
use crate::views::{View, ViewSlot};

const PLAYLIST_COVER_SIZE: u32 = 96;
const CATALOG_ITEM: &str = "catalog";

fn with_model<Output>(
    window: &AppWindow,
    action: impl FnOnce(&PlaylistsModel) -> Output,
) -> Option<Output> {
    let rows = window.global::<PlaylistsState>().get_rows();
    rows.as_any().downcast_ref::<PlaylistsModel>().map(action)
}

fn catalog_slot() -> ViewSlot {
    View::Playlists.slot(CATALOG_ITEM)
}

pub fn load(window: &AppWindow, context: &Context) {
    let Some(server) = context.session.get() else {
        return;
    };
    let Some(generation) = with_model(window, PlaylistsModel::begin) else {
        return;
    };
    context.session.open_view();
    window.global::<PlaylistsState>().set_is_loading(true);
    let weak = window.as_weak();
    let views = context.views(&server);
    let catalog = async move { fetch_all(server.as_ref()).await };
    views.load(Some(catalog_slot()), catalog, move |result| {
        on_ui(&weak, move |window| {
            apply_loaded(window, generation, result);
        });
    });
}

fn apply_loaded(window: &AppWindow, generation: u64, result: cadence_api::Result<Vec<Playlist>>) {
    let playlists = result
        .inspect_err(|error| show_status(window, &tr!("Could not load playlists: {}", error)))
        .unwrap_or_default();
    if with_model(window, |model| model.is_current(generation)) != Some(true) {
        return;
    }
    with_model(window, |model| model.set_playlists(generation, &playlists));
    window.global::<PlaylistsState>().set_is_loading(false);
}

pub fn refresh(window: &AppWindow) {
    if window.global::<AppState>().get_route() == Route::Playlists {
        window.global::<PlaylistsState>().invoke_load("".into());
    }
}

pub fn fetch_cover(weak: slint::Weak<AppWindow>, context: &Context, cover_id: String) {
    let Some(server) = context.session.get() else {
        return;
    };
    let covers = context.view_covers();
    context.runtime.spawn(async move {
        let source = CoverSource::Server {
            server,
            cover_id: cover_id.clone(),
        };
        let cover = source.fetch(covers.as_ref(), PLAYLIST_COVER_SIZE).await;
        on_ui(&weak, move |window| {
            let image = cover.and_then(|cover| to_image(&cover));
            with_model(window, |model| model.apply_cover(cover_id, image));
        });
    });
}
