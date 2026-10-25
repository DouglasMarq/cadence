mod actions;
mod binding;
mod loading;
mod model;
mod trail;

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod view_tests;

use slint::ComponentHandle;

use crate::ui::{AppState, AppWindow, Context, FoldersState, Route};
use trail::{Crumb, SharedTrail};

pub fn install(window: &AppWindow, context: &Context) {
    let trail = SharedTrail::default();
    binding::install_model(window, context);
    install_load(window, context, &trail);
    install_open(window, &trail);
    install_playing(window, context);
}

fn install_load(window: &AppWindow, context: &Context, trail: &SharedTrail) {
    let (weak, load_context, load_trail) = (window.as_weak(), context.clone(), trail.clone());
    window.global::<FoldersState>().on_load(move |detail_id| {
        let Some(window) = weak.upgrade().filter(loading::is_open) else {
            return;
        };
        loading::show_trail(&window, &load_trail, &detail_id);
        loading::load(&window, &load_context, &detail_id);
    });
}

fn install_open(window: &AppWindow, trail: &SharedTrail) {
    let (weak, open_trail) = (window.as_weak(), trail.clone());
    window
        .global::<FoldersState>()
        .on_open(move |folder_id, name| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let parent_id = window.global::<AppState>().get_detail_id();
            open_trail
                .borrow_mut()
                .enter(&parent_id, Crumb::new(&folder_id, &name));
            window
                .global::<AppState>()
                .invoke_navigate(Route::Folders, folder_id);
        });
}

fn install_playing(window: &AppWindow, context: &Context) {
    let state = window.global::<FoldersState>();

    let (weak, folder_context) = (window.as_weak(), context.clone());
    state.on_play_folder(move |folder_id, mode| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        for command in actions::folder_play_commands(&window, &folder_id, mode) {
            folder_context.send(command);
        }
    });

    let (weak, row_context) = (window.as_weak(), context.clone());
    state.on_play_row(move |row| {
        if let (Some(window), Ok(row)) = (weak.upgrade(), usize::try_from(row)) {
            actions::play_row(&window, &row_context, row);
        }
    });

    let (weak, song_context) = (window.as_weak(), context.clone());
    state.on_song_action(move |row, action| {
        let (Some(window), Ok(row)) = (weak.upgrade(), usize::try_from(row)) else {
            return;
        };
        if let Some(command) = actions::song_action_command(&window, row, action) {
            song_context.send(command);
        }
    });
}
