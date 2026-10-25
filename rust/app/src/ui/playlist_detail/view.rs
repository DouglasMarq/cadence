use std::sync::Arc;

use slint::ComponentHandle;

use super::actions::SongAction;
use super::binding::{install_model, with_model};
use super::deps::DetailDeps;
use super::editing::remove;
use super::loading::load;
use super::playing::{PlayStart, play_playlist};
use super::selecting::install_selection;
use crate::ui::playlist_picker::PlaylistPicker;
use crate::ui::{AppWindow, Context, PlaylistDetailState};

pub fn install(window: &AppWindow, context: &Context, picker: &PlaylistPicker) {
    let deps = DetailDeps {
        context: context.clone(),
        shared: Arc::default(),
        picker: picker.clone(),
    };
    install_model(window, context);
    install_load(window, &deps);
    install_play(window, &deps);
    install_play_row(window, &deps);
    install_selection(window);
    install_selection_actions(window, &deps);
    install_context_action(window, &deps);
}

fn install_load(window: &AppWindow, deps: &DetailDeps) {
    let (weak, deps) = (window.as_weak(), deps.clone());
    window
        .global::<PlaylistDetailState>()
        .on_load(move |playlist_id| {
            if let Some(window) = weak.upgrade() {
                load(&window, &deps, &playlist_id);
            }
        });
}

fn install_play(window: &AppWindow, deps: &DetailDeps) {
    let (weak, deps) = (window.as_weak(), deps.clone());
    window
        .global::<PlaylistDetailState>()
        .on_play(move |is_shuffled| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if with_model(&window, |model| model.total()).unwrap_or_default() == 0 {
                return;
            }
            let start = PlayStart::FromTop { is_shuffled };
            play_playlist(&window, &deps, start);
        });
}

fn install_play_row(window: &AppWindow, deps: &DetailDeps) {
    let (weak, deps) = (window.as_weak(), deps.clone());
    window
        .global::<PlaylistDetailState>()
        .on_play_row(move |index| {
            let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
                return;
            };
            let Some(song_id) = with_model(&window, |model| {
                (index < model.total()).then(|| model.song_id_at(index))
            })
            .flatten() else {
                return;
            };
            let start = PlayStart::AtRow { index, song_id };
            play_playlist(&window, &deps, start);
        });
}

fn install_selection_actions(window: &AppWindow, deps: &DetailDeps) {
    let state = window.global::<PlaylistDetailState>();

    let (weak, remove_deps) = (window.as_weak(), deps.clone());
    state.on_remove_selected(move || {
        if let Some(window) = weak.upgrade() {
            let indexes = with_model(&window, |model| model.selected_indexes()).unwrap_or_default();
            remove(&window, &remove_deps, indexes);
        }
    });

    let (weak, add_deps) = (window.as_weak(), deps.clone());
    state.on_add_selected_to_playlist(move || {
        if let Some(window) = weak.upgrade() {
            let indexes = with_model(&window, |model| model.selected_indexes()).unwrap_or_default();
            SongAction::AddToPlaylist.act_on_rows(&window, &add_deps, &indexes);
        }
    });
}

fn install_context_action(window: &AppWindow, deps: &DetailDeps) {
    let (weak, deps) = (window.as_weak(), deps.clone());
    window
        .global::<PlaylistDetailState>()
        .on_context_action(move |index, action| {
            let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
                return;
            };
            let targets = with_model(&window, |model| {
                (index < model.total()).then(|| model.targets(index))
            })
            .flatten();
            let Some(targets) = targets else {
                return;
            };
            match SongAction::from_menu(action) {
                Some(song_action) => song_action.act_on_rows(&window, &deps, &targets),
                None => remove(&window, &deps, targets),
            }
        });
}
