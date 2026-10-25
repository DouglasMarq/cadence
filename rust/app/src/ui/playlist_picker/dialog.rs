use std::sync::Arc;

use cadence_core::model::Playlist;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::choices::{filter_playlists, picker_row};
use super::handle::PlaylistPicker;
use super::picker::{PickedSongs, SharedPicker, lock};
use crate::ui::playlists::fetch_all;
use crate::ui::tracks::ListedSongs;
use crate::ui::{AppWindow, PickerRow, PlaylistPickerState, clamp_i32, on_ui, show_status};

pub fn show_rows(window: &AppWindow, shared: &SharedPicker) {
    let state = window.global::<PlaylistPickerState>();
    let rows: Vec<PickerRow> = filter_playlists(&lock(shared).playlists, &state.get_filter())
        .into_iter()
        .map(picker_row)
        .collect();
    state.set_rows(ModelRc::new(VecModel::from(rows)));
}

pub fn close(window: &AppWindow, shared: &SharedPicker) {
    lock(shared).restart(PickedSongs::default());
    let state = window.global::<PlaylistPickerState>();
    state.set_is_open(false);
    state.set_is_creating(false);
    state.set_is_busy(false);
    state.set_rows(ModelRc::default());
}

impl PlaylistPicker {
    pub fn open_listed(&self, window: &AppWindow, listed: ListedSongs) {
        open(window, self, PickedSongs::Listed(listed));
    }
}

pub fn open(window: &AppWindow, picker: &PlaylistPicker, songs: PickedSongs) {
    let Some(server) = picker.context.session.get() else {
        return;
    };
    let song_count = songs.song_count();
    if song_count == 0 {
        return;
    }
    let generation = lock(&picker.shared).restart(songs);
    show_opened(window, song_count);
    let (weak, shared) = (window.as_weak(), Arc::clone(&picker.shared));
    picker.context.runtime.spawn(async move {
        let result = fetch_all(server.as_ref()).await;
        on_ui(&weak, move |window| {
            apply_playlists(window, &shared, LoadedPlaylists { generation, result });
        });
    });
}

fn show_opened(window: &AppWindow, song_count: usize) {
    let state = window.global::<PlaylistPickerState>();
    state.set_song_count(clamp_i32(song_count as u64));
    state.set_filter("".into());
    state.set_rows(ModelRc::default());
    state.set_is_creating(false);
    state.set_is_busy(false);
    state.set_is_loading(true);
    state.set_is_open(true);
}

struct LoadedPlaylists {
    generation: u64,
    result: cadence_api::Result<Vec<Playlist>>,
}

fn apply_playlists(window: &AppWindow, shared: &SharedPicker, loaded: LoadedPlaylists) {
    let mut picker = lock(shared);
    if !picker.is_current(loaded.generation) {
        return;
    }
    let loaded = loaded.result.map(|playlists| picker.playlists = playlists);
    drop(picker);
    if let Err(error) = loaded {
        show_status(window, &tr!("Could not load playlists: {}", error));
    }
    window.global::<PlaylistPickerState>().set_is_loading(false);
    show_rows(window, shared);
}
