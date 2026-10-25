use std::sync::Arc;

use cadence_core::model::Song;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::deps::DetailDeps;
use super::detail::{RemovalTarget, SharedDetail, lock};
use super::loading::refresh_if_showing;
use crate::ui::{Actions, AppWindow, PlaylistDetailState, on_ui, show_status};

pub fn remove(window: &AppWindow, deps: &DetailDeps, indexes: Vec<usize>) {
    let Some(server) = deps.context.session.get() else {
        return;
    };
    let Some(target) = lock(&deps.shared).start_removal(&indexes) else {
        return;
    };
    window.global::<PlaylistDetailState>().set_is_busy(true);
    let (weak, shared) = (window.as_weak(), Arc::clone(&deps.shared));
    deps.context.runtime.spawn(async move {
        let result = server
            .remove_from_playlist(&target.playlist_id, &indexes)
            .await;
        on_ui(&weak, move |window| {
            let finished = FinishedRemoval {
                target,
                removed: indexes.len(),
                result,
            };
            finish_removal(window, &shared, finished);
        });
    });
}

struct FinishedRemoval {
    target: RemovalTarget,
    removed: usize,
    result: cadence_api::Result<()>,
}

fn finish_removal(window: &AppWindow, shared: &SharedDetail, finished: FinishedRemoval) {
    let FinishedRemoval {
        target,
        removed,
        result,
    } = finished;
    let mut detail = lock(shared);
    if !detail.is_current(target.generation) {
        return;
    }
    detail.finish_removal(target.generation);
    drop(detail);
    window.global::<PlaylistDetailState>().set_is_busy(false);
    match result {
        Ok(()) => {
            let message = tr_plural!(
                "Removed {n} song from \"{}\"",
                "Removed {n} songs from \"{}\"",
                removed,
                target.name
            );
            show_status(window, &message);
            refresh_if_showing(window, &target.playlist_id);
        }
        Err(error) => show_status(window, &tr!("Could not remove songs: {}", error)),
    }
}

pub fn add_to_playlist(window: &AppWindow, songs: &[Song]) {
    if songs.is_empty() {
        return;
    }
    let song_ids: Vec<SharedString> = songs.iter().map(|song| song.id.as_str().into()).collect();
    window
        .global::<Actions>()
        .invoke_add_to_playlist(ModelRc::new(VecModel::from(song_ids)));
}
