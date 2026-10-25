use slint::ComponentHandle;

use super::actions::{SongAction, SongActions};
use super::model::TracksModel;
use super::row::primary_artist_id;
use super::with_model;
use crate::ui::{AppWindow, Route, TracksState, clamp_i32, rating_stars};

fn update_selected_count(window: &AppWindow, change: impl FnOnce(&TracksModel)) {
    with_model(window, change);
    let Some(count) = with_model(window, TracksModel::selected_count) else {
        return;
    };
    window
        .global::<TracksState>()
        .set_selected_count(clamp_i32(count as u64));
}

fn install_row_selection(window: &AppWindow) {
    let state = window.global::<TracksState>();

    let weak = window.as_weak();
    state.on_select_row(move |index, is_toggling, is_extending| {
        let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
            return;
        };
        update_selected_count(&window, |model| {
            model.select(index, is_toggling, is_extending)
        });
    });

    let weak = window.as_weak();
    state.on_context_row(move |index| {
        let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
            return;
        };
        update_selected_count(&window, |model| model.focus(index));
    });

    let weak = window.as_weak();
    state.on_select_all(move || {
        if let Some(window) = weak.upgrade() {
            update_selected_count(&window, TracksModel::select_all);
        }
    });

    let weak = window.as_weak();
    state.on_clear_selection(move || {
        if let Some(window) = weak.upgrade() {
            update_selected_count(&window, TracksModel::clear_selection);
        }
    });
}

fn action_callback(
    window: &AppWindow,
    actions: &SongActions,
    action: SongAction,
) -> impl Fn() + 'static {
    let weak = window.as_weak();
    let actions = actions.clone();
    move || {
        if let Some(window) = weak.upgrade() {
            actions.act_on_selection(&window, action);
        }
    }
}

fn install_selection_actions(window: &AppWindow, actions: &SongActions) {
    let state = window.global::<TracksState>();
    state.on_play_selected(action_callback(window, actions, SongAction::Play));
    state.on_add_selected_to_playlist(action_callback(window, actions, SongAction::AddToPlaylist));

    let play_next = action_callback(window, actions, SongAction::PlayNext);
    let add_last = action_callback(window, actions, SongAction::AddLast);
    state.on_queue_selected(move |is_play_next| match is_play_next {
        true => play_next(),
        false => add_last(),
    });

    let add_favorite = action_callback(window, actions, SongAction::Favorite(true));
    let remove_favorite = action_callback(window, actions, SongAction::Favorite(false));
    state.on_favorite_selected(move |is_favorite| match is_favorite {
        true => add_favorite(),
        false => remove_favorite(),
    });

    let weak = window.as_weak();
    let rate_actions = actions.clone();
    state.on_rate_selected(move |rating| {
        if let Some(window) = weak.upgrade() {
            rate_actions.act_on_selection(&window, SongAction::Rate(rating_stars(rating)));
        }
    });
}

fn install_navigation(window: &AppWindow, actions: &SongActions) {
    let state = window.global::<TracksState>();

    let weak = window.as_weak();
    let album_actions = actions.clone();
    state.on_go_to_album(move || {
        if let Some(window) = weak.upgrade() {
            album_actions.go_to(&window, Route::AlbumDetail, |song| song.album_id.clone());
        }
    });

    let weak = window.as_weak();
    let artist_actions = actions.clone();
    state.on_go_to_artist(move || {
        if let Some(window) = weak.upgrade() {
            artist_actions.go_to(&window, Route::ArtistDetail, |song| {
                primary_artist_id(song).map(str::to_owned)
            });
        }
    });
}

fn install_row_edits(window: &AppWindow, actions: &SongActions) {
    let state = window.global::<TracksState>();

    let weak = window.as_weak();
    let favorite_actions = actions.clone();
    state.on_toggle_favorite(move |index| {
        let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
            return;
        };
        if let Some(song) = with_model(&window, |model| model.song_at(index)).flatten() {
            favorite_actions.set_favorite(&window, vec![song.id], !song.is_favorite);
        }
    });

    let weak = window.as_weak();
    let rating_actions = actions.clone();
    state.on_rate_row(move |index, rating| {
        let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) else {
            return;
        };
        if let Some(id) = with_model(&window, |model| model.song_id_at(index)).flatten() {
            rating_actions.set_rating(&window, vec![id], rating_stars(rating));
        }
    });
}

pub(super) fn install(window: &AppWindow, actions: &SongActions) {
    install_row_selection(window);
    install_selection_actions(window, actions);
    install_navigation(window, actions);
    install_row_edits(window, actions);
}
