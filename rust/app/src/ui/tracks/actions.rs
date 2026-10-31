use cadence_api::ApiError;
use cadence_core::model::{ItemKind, Song};
use slint::{ComponentHandle, SharedString};

use super::model::TracksModel;
use super::selection::{FirstSelected, SelectedSongs};
use super::{string_model, with_model};
use crate::controller::Command;
use crate::ui::playlist_picker::PlaylistPicker;
use crate::ui::{Actions, AppState, AppWindow, Context, Route, on_ui, show_status};
use crate::views::Views;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SongAction {
    Play,
    PlayNext,
    AddLast,
    AddToPlaylist,
    Favorite(bool),
    Rate(u8),
}

#[derive(Clone)]
pub struct SongActions {
    pub context: Context,
    pub picker: PlaylistPicker,
}

fn open_playlist_picker(window: &AppWindow, songs: &[Song]) {
    let ids: Vec<SharedString> = songs.iter().map(|song| song.id.as_str().into()).collect();
    window
        .global::<Actions>()
        .invoke_add_to_playlist(string_model(ids));
}

fn ids_of(songs: &[Song]) -> Vec<String> {
    songs.iter().map(|song| song.id.clone()).collect()
}

pub(super) async fn rate_until_failure(
    views: &Views,
    ids: Vec<String>,
    rating: u8,
) -> (Vec<String>, Option<ApiError>) {
    let mut rated = Vec::new();
    for id in ids {
        if let Err(error) = views.set_rating(ItemKind::Song, &id, rating).await {
            return (rated, Some(error));
        }
        rated.push(id);
    }
    (rated, None)
}

impl SongActions {
    pub(super) fn act_on_selection(&self, window: &AppWindow, action: SongAction) {
        match with_model(window, TracksModel::selected_songs) {
            Some(SelectedSongs::Loaded(songs)) => self.perform(window, action, songs),
            Some(SelectedSongs::Listed(listed)) => self.act_on_listed(window, action, listed),
            None => {}
        }
    }

    fn perform(&self, window: &AppWindow, action: SongAction, songs: Vec<Song>) {
        if songs.is_empty() {
            return;
        }
        let context = &self.context;
        match action {
            SongAction::Play => context.send(Command::PlaySongs {
                songs,
                song_id: None,
            }),
            SongAction::PlayNext => context.send(Command::AddNext(songs)),
            SongAction::AddLast => context.send(Command::AddLast(songs)),
            SongAction::AddToPlaylist => open_playlist_picker(window, &songs),
            SongAction::Favorite(favorite) => self.set_favorite(window, ids_of(&songs), favorite),
            SongAction::Rate(rating) => self.set_rating(window, ids_of(&songs), rating),
        }
    }

    pub(super) fn set_favorite(&self, window: &AppWindow, ids: Vec<String>, is_favorite: bool) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let weak = window.as_weak();
        let views = self.context.views(&server);
        self.context.runtime.spawn(async move {
            let result = views.set_favorite(ItemKind::Song, &ids, is_favorite).await;
            on_ui(&weak, move |window| match result {
                Ok(()) => {
                    with_model(window, |model| {
                        model.update_songs(&ids, |song| song.is_favorite = is_favorite);
                    });
                }
                Err(error) => show_status(window, &tr!("Could not update favorites: {}", error)),
            });
        });
    }

    pub(super) fn set_rating(&self, window: &AppWindow, ids: Vec<String>, rating: u8) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let weak = window.as_weak();
        let work = self.context.session.background_work();
        let views = self.context.views(&server);
        self.context.runtime.spawn(async move {
            let Some((rated, failure)) = work.run(rate_until_failure(&views, ids, rating)).await
            else {
                return;
            };
            on_ui(&weak, move |window| {
                let user_rating = (rating > 0).then_some(rating);
                with_model(window, |model| {
                    model.update_songs(&rated, |song| song.user_rating = user_rating);
                });
                if let Some(error) = failure {
                    show_status(window, &tr!("Could not set the rating: {}", error));
                }
            });
        });
    }

    pub(super) fn go_to(
        &self,
        window: &AppWindow,
        route: Route,
        target: impl FnOnce(&Song) -> Option<String> + Send + 'static,
    ) {
        let Some(first) = with_model(window, TracksModel::first_selected).flatten() else {
            return;
        };
        self.with_first_song(window, first, move |window, song| {
            if let Some(target_id) = target(&song) {
                window
                    .global::<AppState>()
                    .invoke_navigate(route, target_id.into());
            }
        });
    }

    fn with_first_song(
        &self,
        window: &AppWindow,
        first: FirstSelected,
        use_song: impl FnOnce(&AppWindow, Song) + Send + 'static,
    ) {
        if let Some(song) = first.song {
            use_song(window, song);
            return;
        }
        let Some(server) = self.context.session.get() else {
            return;
        };
        let weak = window.as_weak();
        self.context.runtime.spawn(async move {
            let mut chunks = first.listed.into_chunks(1);
            let result = chunks.next_songs(server.as_ref()).await;
            on_ui(&weak, move |window| match result {
                Ok(songs) => {
                    if let Some(song) = songs.into_iter().flatten().next() {
                        use_song(window, song);
                    }
                }
                Err(error) => show_status(window, &tr!("Could not load the selection: {}", error)),
            });
        });
    }
}
