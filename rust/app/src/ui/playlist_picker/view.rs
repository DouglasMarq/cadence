use std::sync::Arc;

use slint::{ComponentHandle, Model};

use super::dialog::{close, open, show_rows};
use super::handle::PlaylistPicker;
use super::picker::{PickedSongs, SharedPicker};
use super::requests::{create, pick};
use crate::ui::{Actions, AppWindow, Context, PlaylistPickerState};

pub fn install(window: &AppWindow, context: &Context) -> PlaylistPicker {
    let shared: SharedPicker = Arc::default();
    let picker = PlaylistPicker::new(Arc::clone(&shared), context.clone());
    install_open(window, &picker);
    install_filter(window, &shared);
    install_pick(window, &picker);
    install_create(window, &picker);
    install_cancel(window, &shared);
    picker
}

fn install_open(window: &AppWindow, picker: &PlaylistPicker) {
    let (weak, picker) = (window.as_weak(), picker.clone());
    window
        .global::<Actions>()
        .on_add_to_playlist(move |song_ids| {
            if let Some(window) = weak.upgrade() {
                let song_ids = song_ids.iter().map(|song_id| song_id.to_string()).collect();
                open(&window, &picker, PickedSongs::Ids(song_ids));
            }
        });
}

fn install_filter(window: &AppWindow, shared: &SharedPicker) {
    let (weak, filter_shared) = (window.as_weak(), Arc::clone(shared));
    window
        .global::<PlaylistPickerState>()
        .on_filter_edited(move |_| {
            if let Some(window) = weak.upgrade() {
                show_rows(&window, &filter_shared);
            }
        });
}

fn install_pick(window: &AppWindow, picker: &PlaylistPicker) {
    let (weak, picker) = (window.as_weak(), picker.clone());
    window
        .global::<PlaylistPickerState>()
        .on_pick(move |playlist_id| {
            if let Some(window) = weak.upgrade() {
                pick(&window, &picker, playlist_id.to_string());
            }
        });
}

fn install_create(window: &AppWindow, picker: &PlaylistPicker) {
    let (weak, picker) = (window.as_weak(), picker.clone());
    window
        .global::<PlaylistPickerState>()
        .on_create(move |name| {
            if let Some(window) = weak.upgrade() {
                create(&window, &picker, &name);
            }
        });
}

fn install_cancel(window: &AppWindow, shared: &SharedPicker) {
    let (weak, cancel_shared) = (window.as_weak(), Arc::clone(shared));
    window.global::<PlaylistPickerState>().on_cancel(move || {
        if let Some(window) = weak.upgrade() {
            close(&window, &cancel_shared);
        }
    });
}
