use std::rc::Rc;

use cadence_core::model::{Page, Song};
use slint::{ComponentHandle, Model, ModelRc};

use super::model::{PageRequest, PlaylistSongsModel};
use super::pages::paging_for_page;
use crate::ui::{AppWindow, Context, PlaylistDetailState, on_ui, show_status};
use crate::views::View;

pub fn with_model<Output>(
    window: &AppWindow,
    action: impl FnOnce(&PlaylistSongsModel) -> Output,
) -> Option<Output> {
    let rows = window.global::<PlaylistDetailState>().get_rows();
    rows.as_any()
        .downcast_ref::<PlaylistSongsModel>()
        .map(action)
}

pub fn install_model(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let fetch_context = context.clone();
    let model = Rc::new(PlaylistSongsModel::new(move |request| {
        spawn_page_load(weak.clone(), &fetch_context, request);
    }));
    window
        .global::<PlaylistDetailState>()
        .set_rows(ModelRc::from(model));
}

fn spawn_page_load(window: slint::Weak<AppWindow>, context: &Context, request: PageRequest) {
    let Some(server) = context.session.get() else {
        return;
    };
    let views = context.views(&server);
    let playlist_id = request.playlist_id.clone();
    let slot = View::Playlist(playlist_id.clone()).slot(format!("page/{}", request.page));
    let paging = paging_for_page(request.page);
    let songs = async move { server.playlist_song_page(&playlist_id, paging).await };
    views.load(Some(slot), songs, move |result| {
        let request = request.clone();
        on_ui(&window, move |window| show_page(window, &request, result));
    });
}

fn show_page(window: &AppWindow, request: &PageRequest, result: cadence_api::Result<Page<Song>>) {
    match result {
        Ok(songs) => {
            let changed = with_model(window, |model| {
                model.reported_total_change(request, songs.total_count)
            });
            with_model(window, |model| model.apply_requested_page(request, songs));
            if let Some(Some(_)) = changed {
                window.global::<PlaylistDetailState>().set_selected_count(0);
            }
        }
        Err(error) => {
            let is_current = with_model(window, |model| model.is_current(request));
            if is_current != Some(true) {
                return;
            }
            with_model(window, |model| model.page_failed(request));
            show_status(window, &tr!("Could not load the playlist: {}", error));
        }
    }
}
