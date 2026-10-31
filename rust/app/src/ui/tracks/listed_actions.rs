use cadence_core::model::{ItemKind, Song};
use slint::ComponentHandle;

use super::actions::{SongAction, SongActions, rate_until_failure};
use super::listed::{LISTED_CHUNK_SIZE, ListedSongs};
use super::with_model;
use crate::controller::Command;
use crate::list_stream::Placement;
use crate::ui::{AppWindow, Context, on_ui, show_status};
use crate::views::Views;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListedEdit {
    Favorite(bool),
    Rate(u8),
}

impl ListedEdit {
    async fn apply(self, views: &Views, song_ids: Vec<String>) -> (Vec<String>, Option<String>) {
        match self {
            Self::Favorite(favorite) => {
                match views
                    .set_favorite(ItemKind::Song, &song_ids, favorite)
                    .await
                {
                    Ok(()) => (song_ids, None),
                    Err(error) => (
                        Vec::new(),
                        Some(tr!("Could not update favorites: {}", error)),
                    ),
                }
            }
            Self::Rate(rating) => {
                let (rated, failure) = rate_until_failure(views, song_ids, rating).await;
                let message = failure.map(|error| tr!("Could not set the rating: {}", error));
                (rated, message)
            }
        }
    }

    fn update(self, song: &mut Song) {
        match self {
            Self::Favorite(favorite) => song.is_favorite = favorite,
            Self::Rate(rating) => song.user_rating = (rating > 0).then_some(rating),
        }
    }

    async fn edit_listed(
        self,
        views: Views,
        weak: &slint::Weak<AppWindow>,
        listed: ListedSongs,
    ) -> Option<String> {
        let mut chunks = listed.into_chunks(LISTED_CHUNK_SIZE);
        loop {
            let song_ids = match chunks.next_ids(views.server()).await {
                Ok(Some(song_ids)) => song_ids,
                Ok(None) => return None,
                Err(error) => return Some(tr!("Could not load the selection: {}", error)),
            };
            let (edited, failure) = self.apply(&views, song_ids).await;
            show_edited(weak, self, edited);
            if failure.is_some() {
                return failure;
            }
        }
    }
}

impl SongActions {
    pub(super) fn act_on_listed(
        &self,
        window: &AppWindow,
        action: SongAction,
        listed: ListedSongs,
    ) {
        match action {
            SongAction::Play => enqueue(&self.context, listed, Placement::Now),
            SongAction::PlayNext => enqueue(&self.context, listed, Placement::Next),
            SongAction::AddLast => enqueue(&self.context, listed, Placement::Last),
            SongAction::AddToPlaylist => self.picker.open_listed(window, listed),
            SongAction::Favorite(favorite) => {
                self.spawn_edit(window, listed, ListedEdit::Favorite(favorite));
            }
            SongAction::Rate(rating) => self.spawn_edit(window, listed, ListedEdit::Rate(rating)),
        }
    }

    fn spawn_edit(&self, window: &AppWindow, listed: ListedSongs, edit: ListedEdit) {
        let Some(server) = self.context.session.get() else {
            return;
        };
        let weak = window.as_weak();
        let work = self.context.session.background_work();
        let views = self.context.views(&server);
        self.context.runtime.spawn(async move {
            let edited = work.run(edit.edit_listed(views, &weak, listed)).await;
            if let Some(Some(message)) = edited {
                on_ui(&weak, move |window| show_status(window, &message));
            }
        });
    }
}

fn enqueue(context: &Context, listed: ListedSongs, placement: Placement) {
    context.send(Command::EnqueueFromList {
        source: listed.source,
        ranges: listed.ranges.into_ranges(),
        placement,
    });
}

fn show_edited(weak: &slint::Weak<AppWindow>, edit: ListedEdit, edited: Vec<String>) {
    on_ui(weak, move |window| {
        with_model(window, |model| {
            model.update_songs(&edited, |song| edit.update(song));
        });
    });
}
