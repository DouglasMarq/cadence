use cadence_core::model::{Folder, FolderId, Song};
use slint::{ComponentHandle, ModelRc, VecModel};

use super::binding::{first_song_page, with_model};
use super::model::FirstSongs;
use super::trail::{Crumb, SharedTrail, root_name};
use crate::ui::{
    AppState, AppWindow, Context, FolderCrumb, FoldersState, Route, on_ui, show_status,
};
use crate::views::{View, ViewSlot};

const SUBFOLDERS_ITEM: &str = "subfolders";
const FIRST_SONGS_ITEM: &str = "first-songs";

type FirstPage = (Option<usize>, Vec<Song>);

fn folder_slot(item: &str, folder: &FolderId) -> ViewSlot {
    View::Folders.slot(format!("{item}/{}", folder.detail_id()))
}

pub fn is_open(window: &AppWindow) -> bool {
    window.global::<AppState>().get_route() == Route::Folders
}

pub fn show_trail(window: &AppWindow, trail: &SharedTrail, detail_id: &str) {
    let crumbs = trail.borrow_mut().path_to(detail_id);
    show_crumbs(window, detail_id, &crumbs);
}

pub fn load(window: &AppWindow, context: &Context, detail_id: &str) {
    let folder = FolderId::from_detail_id(detail_id);
    let Some(generation) = with_model(window, |model| model.reset(folder.clone())) else {
        return;
    };
    context.session.open_view();
    let state = window.global::<FoldersState>();
    state.set_summary("".into());
    state.set_error("".into());
    let Some(server) = context.session.get() else {
        state.set_is_loading(false);
        return;
    };
    state.set_is_loading(true);
    let views = context.views(&server);
    let weak = window.as_weak();
    let subfolders_slot = folder_slot(SUBFOLDERS_ITEM, &folder);
    let (subfolders_server, subfolders_folder) = (server.clone(), folder.clone());
    let subfolders = async move { subfolders_server.subfolders(&subfolders_folder).await };
    views.load(Some(subfolders_slot), subfolders, move |result| {
        on_ui(&weak, move |window| {
            apply_subfolders(window, generation, result);
        });
    });
    let weak = window.as_weak();
    let songs_slot = folder_slot(FIRST_SONGS_ITEM, &folder);
    let first_songs = async move {
        let page = server.folder_song_page(&folder, first_song_page()).await?;
        cadence_api::Result::Ok((page.total_count, page.items))
    };
    views.load(Some(songs_slot), first_songs, move |result| {
        on_ui(&weak, move |window| {
            apply_first_songs(window, generation, result);
        });
    });
}

fn show_crumbs(window: &AppWindow, detail_id: &str, crumbs: &[Crumb]) {
    let state = window.global::<FoldersState>();
    let rows: Vec<FolderCrumb> = crumbs
        .iter()
        .map(|crumb| FolderCrumb {
            id: crumb.id.as_str().into(),
            name: crumb.name.as_str().into(),
        })
        .collect();
    let name = crumbs
        .last()
        .map_or_else(root_name, |crumb| crumb.name.clone());
    state.set_name(name.into());
    state.set_folder_id(detail_id.into());
    state.set_crumbs(ModelRc::new(VecModel::from(rows)));
}

fn is_showing(window: &AppWindow, generation: u64) -> bool {
    with_model(window, |model| model.is_current(generation)) == Some(true)
}

fn show_failure(window: &AppWindow, error: &cadence_api::ApiError) {
    let message = tr!("Could not open the folder: {}", error);
    window
        .global::<FoldersState>()
        .set_error(message.as_str().into());
    show_status(window, &message);
}

fn apply_subfolders(window: &AppWindow, generation: u64, result: cadence_api::Result<Vec<Folder>>) {
    if !is_showing(window, generation) {
        return;
    }
    let folders = result
        .inspect_err(|error| show_failure(window, error))
        .unwrap_or_default();
    with_model(window, |model| model.show_folders(generation, folders));
    show_progress(window);
}

fn apply_first_songs(window: &AppWindow, generation: u64, result: cadence_api::Result<FirstPage>) {
    if !is_showing(window, generation) {
        return;
    }
    let (reported_total, songs) = result
        .inspect_err(|error| show_failure(window, error))
        .unwrap_or_default();
    let first_songs = FirstSongs {
        song_total: reported_total.unwrap_or(songs.len()),
        songs,
    };
    with_model(window, |model| {
        model.show_first_songs(generation, first_songs)
    });
    show_progress(window);
}

fn show_progress(window: &AppWindow) {
    let Some((is_settled, (folder_count, song_count))) =
        with_model(window, |model| (model.is_settled(), model.counts()))
    else {
        return;
    };
    let state = window.global::<FoldersState>();
    state.set_is_loading(!is_settled);
    state.set_summary(summary(folder_count, song_count).into());
}

fn summary(folder_count: usize, song_count: usize) -> String {
    [
        (folder_count > 0).then(|| tr_plural!("{n} folder", "{n} folders", folder_count)),
        (song_count > 0).then(|| tr_plural!("{n} song", "{n} songs", song_count)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

#[cfg(test)]
mod tests {
    use super::summary;

    #[test]
    fn summaries_skip_empty_counts() {
        assert_eq!(summary(400, 0), "400 folders");
        assert_eq!(summary(1, 1), "1 folder, 1 song");
        assert_eq!(summary(0, 12), "12 songs");
        assert_eq!(summary(0, 0), "");
    }
}
