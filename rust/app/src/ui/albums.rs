use std::rc::Rc;
use std::time::Duration;

use cadence_core::model::{AlbumListQuery, AlbumSort};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use super::album_grid::{
    AlbumAction, AlbumGridModel, AlbumRequest, GridTarget, album_action, spawn_request, with_model,
};
use super::{
    AlbumGridRow, AlbumsState, AppWindow, Context, ListControls, clamp_i32, normalized_search,
    show_status, sort_at, sort_order,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

pub const SORTS: [AlbumSort; 11] = [
    AlbumSort::Name,
    AlbumSort::AlbumArtist,
    AlbumSort::Year,
    AlbumSort::RecentlyAdded,
    AlbumSort::RecentlyPlayed,
    AlbumSort::PlayCount,
    AlbumSort::Rating,
    AlbumSort::Favorited,
    AlbumSort::SongCount,
    AlbumSort::Duration,
    AlbumSort::Random,
];

fn sort_label(sort: AlbumSort) -> String {
    match sort {
        AlbumSort::Name => tr!("Name"),
        AlbumSort::AlbumArtist => tr!("Album artist"),
        AlbumSort::Year => tr!("Release year"),
        AlbumSort::RecentlyAdded => tr!("Recently added"),
        AlbumSort::RecentlyPlayed => tr!("Recently played"),
        AlbumSort::PlayCount => tr!("Play count"),
        AlbumSort::Rating => tr!("Rating"),
        AlbumSort::Favorited => tr!("Favorited"),
        AlbumSort::SongCount => tr!("Song count"),
        AlbumSort::Duration => tr!("Duration"),
        AlbumSort::Random => tr!("Random"),
    }
}

pub fn album_query(controls: &ListControls) -> AlbumListQuery {
    AlbumListQuery {
        sort: sort_at(&SORTS, controls.sort_index, AlbumSort::Name),
        order: sort_order(controls.is_descending),
        search: normalized_search(controls.search),
        is_favorite: controls.is_favorites_only.then_some(true),
        ..AlbumListQuery::default()
    }
}

fn locate(window: &AppWindow) -> ModelRc<AlbumGridRow> {
    window.global::<AlbumsState>().get_rows()
}

fn current_query(window: &AppWindow) -> AlbumListQuery {
    let state = window.global::<AlbumsState>();
    album_query(&ListControls {
        sort_index: state.get_sort(),
        is_descending: state.get_is_sort_descending(),
        search: &state.get_search(),
        is_favorites_only: state.get_is_favorites_only(),
    })
}

fn reload(window: &AppWindow) {
    let query = current_query(window);
    window.global::<AlbumsState>().set_is_loading(true);
    with_model(window, locate, |model| model.reset(query));
}

fn on_count(window: &AppWindow, result: anyhow::Result<usize>) {
    let state = window.global::<AlbumsState>();
    state.set_is_loading(false);
    match result {
        Ok(total) => state.set_total(clamp_i32(total as u64)),
        Err(error) => {
            state.set_total(0);
            show_status(
                window,
                &tr!("Could not load albums: {}", format!("{error:#}")),
            );
        }
    }
}

pub fn show_sort_options(window: &AppWindow) {
    let labels: Vec<SharedString> = SORTS.iter().map(|sort| sort_label(*sort).into()).collect();
    window
        .global::<AlbumsState>()
        .set_sort_options(ModelRc::new(VecModel::from(labels)));
}

fn install_grid_model(window: &AppWindow, context: &Context) {
    let weak_window = window.as_weak();
    let context = context.clone();
    let model = AlbumGridModel::new(move |request| {
        spawn_request(
            GridTarget {
                window: weak_window.clone(),
                locate,
                on_count,
            },
            &context,
            request,
        )
    });
    window.global::<AlbumsState>().set_rows(ModelRc::new(model));
}

fn install_reload_triggers(window: &AppWindow) {
    let state = window.global::<AlbumsState>();
    let weak_window = window.as_weak();
    state.on_load(move |_| {
        if let Some(window) = weak_window.upgrade() {
            with_model(&window, locate, AlbumGridModel::clear_covers);
            reload(&window);
        }
    });

    let weak_window = window.as_weak();
    state.on_query_changed(move || {
        if let Some(window) = weak_window.upgrade() {
            reload(&window);
        }
    });

    let weak_window = window.as_weak();
    let debounce = Rc::new(Timer::default());
    state.on_search_edited(move || {
        let weak_window = weak_window.clone();
        debounce.start(TimerMode::SingleShot, SEARCH_DEBOUNCE, move || {
            if let Some(window) = weak_window.upgrade() {
                reload(&window);
            }
        });
    });
}

fn install_columns_changed(window: &AppWindow) {
    let weak_window = window.as_weak();
    window
        .global::<AlbumsState>()
        .on_columns_changed(move |columns| {
            if let Some(window) = weak_window.upgrade() {
                with_model(&window, locate, |model| {
                    model.set_columns(usize::try_from(columns).unwrap_or(1))
                });
            }
        });
}

fn install_album_actions(window: &AppWindow, context: &Context) {
    let bind = |action: AlbumAction| {
        let weak_window = window.as_weak();
        let context = context.clone();
        move |album_id: SharedString| {
            if let Some(window) = weak_window.upgrade() {
                album_action(
                    &window,
                    &context,
                    AlbumRequest {
                        album_id: album_id.into(),
                        action,
                    },
                );
            }
        }
    };
    let state = window.global::<AlbumsState>();
    state.on_play(bind(AlbumAction::Play));
    state.on_play_next(bind(AlbumAction::PlayNext));
    state.on_add_to_queue(bind(AlbumAction::AddToQueue));
    state.on_add_to_playlist(bind(AlbumAction::AddToPlaylist));
}

pub fn install(window: &AppWindow, context: &Context) {
    show_sort_options(window);
    install_grid_model(window, context);
    install_reload_triggers(window);
    install_columns_changed(window);
    install_album_actions(window, context);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cadence_core::model::SortOrder;
    use slint::Model;

    use super::*;
    use crate::controller::Command;
    use crate::demo::FakeServer;
    use crate::ui::test_window::TestWindow;
    use crate::ui::{AppState, PlaylistPickerState, Route};

    const COLUMNS: i32 = 4;

    fn open_albums() -> Option<TestWindow> {
        let view = TestWindow::open(Arc::new(FakeServer::new(240)))?;
        view.app
            .global::<AppState>()
            .invoke_navigate(Route::Albums, "".into());
        let state = view.app.global::<AlbumsState>();
        assert!(view.pump(|| !state.get_is_loading() && state.get_total() > 0));
        std::iter::from_fn(|| view.next_command()).for_each(drop);
        Some(view)
    }

    fn wait_for_command(view: &TestWindow) -> Option<Command> {
        let command = std::cell::RefCell::new(None);
        view.pump(|| {
            let mut received = command.borrow_mut();
            if received.is_none() {
                *received = view.next_command();
            }
            received.is_some()
        });
        command.into_inner()
    }

    #[test]
    fn given_the_albums_page_when_opened_then_it_counts_and_lays_out_the_albums() {
        let Some(view) = open_albums() else {
            return;
        };
        let state = view.app.global::<AlbumsState>();

        assert_eq!(state.get_sort_options().row_count(), SORTS.len());
        state.invoke_columns_changed(COLUMNS);

        let rows = state.get_rows();
        assert!(rows.row_count() > 0);
        assert!(view.pump(|| {
            rows.row_data(0)
                .is_some_and(|row| row.cards.row_count() == COLUMNS as usize)
        }));
    }

    #[test]
    fn given_a_search_when_typed_then_the_albums_reload_after_a_pause() {
        let Some(view) = open_albums() else {
            return;
        };
        let state = view.app.global::<AlbumsState>();
        let everything = state.get_total();

        state.set_search("no album is called like this".into());
        state.invoke_search_edited();

        assert!(view.pump(|| !state.get_is_loading() && state.get_total() < everything));

        state.set_search("".into());
        state.set_is_favorites_only(true);
        state.invoke_query_changed();

        assert!(state.get_is_loading());
        assert!(view.pump(|| !state.get_is_loading() && state.get_total() > 0));
        assert!(state.get_total() < everything);
    }

    #[test]
    fn given_an_album_card_when_its_actions_are_used_then_the_album_is_queued() {
        let Some(view) = open_albums() else {
            return;
        };
        let state = view.app.global::<AlbumsState>();

        state.invoke_play("album-0".into());
        let played = wait_for_command(&view);
        state.invoke_play_next("album-0".into());
        let next = wait_for_command(&view);
        state.invoke_add_to_queue("album-0".into());
        let last = wait_for_command(&view);

        assert!(matches!(played, Some(Command::PlaySongs { songs, .. }) if !songs.is_empty()));
        assert!(matches!(next, Some(Command::AddNext(songs)) if !songs.is_empty()));
        assert!(matches!(last, Some(Command::AddLast(songs)) if !songs.is_empty()));
    }

    #[test]
    fn given_an_album_card_when_added_to_a_playlist_then_the_picker_opens() {
        let Some(view) = open_albums() else {
            return;
        };
        let picker = view.app.global::<PlaylistPickerState>();

        view.app
            .global::<AlbumsState>()
            .invoke_add_to_playlist("album-0".into());

        assert!(view.pump(|| picker.get_is_open()));
    }

    #[test]
    fn given_a_missing_album_when_played_then_the_status_says_so() {
        let Some(view) = open_albums() else {
            return;
        };

        view.app
            .global::<AlbumsState>()
            .invoke_play("album-missing".into());

        assert!(view.pump(|| view.status().starts_with("Could not load the album")));
    }

    #[test]
    fn queries_follow_the_controls() {
        let query = album_query(&ListControls {
            sort_index: 2,
            is_descending: true,
            search: "  river ",
            is_favorites_only: true,
        });
        assert_eq!(query.sort, AlbumSort::Year);
        assert_eq!(query.order, SortOrder::Desc);
        assert_eq!(query.search.as_deref(), Some("river"));
        assert_eq!(query.is_favorite, Some(true));

        let query = album_query(&ListControls {
            sort_index: 99,
            is_descending: false,
            search: " ",
            is_favorites_only: false,
        });
        assert_eq!(query, AlbumListQuery::default());
        let unknown_sort = ListControls {
            sort_index: -1,
            is_descending: false,
            search: "",
            is_favorites_only: false,
        };
        assert_eq!(album_query(&unknown_sort).sort, AlbumSort::Name);
    }
}
