use cadence_core::model::{AlbumListQuery, ArtistListQuery, SongListQuery};
use slint::ComponentHandle;

use super::genres::{Binding, CardSource, card_grid, set_grid_columns, with_grid, with_table};
use super::{AppWindow, CardRow, Context, FavoritesState, TrackItem};

pub fn favorite_song_query() -> SongListQuery {
    SongListQuery {
        is_favorite: Some(true),
        ..SongListQuery::default()
    }
}

pub fn favorite_album_query() -> AlbumListQuery {
    AlbumListQuery {
        is_favorite: Some(true),
        ..AlbumListQuery::default()
    }
}

pub fn favorite_artist_query() -> ArtistListQuery {
    ArtistListQuery {
        is_favorite: Some(true),
        ..ArtistListQuery::default()
    }
}

const TRACKS: Binding<TrackItem> = Binding {
    rows: |window| window.global::<FavoritesState>().get_tracks(),
    loading: |window, is_loading| {
        window
            .global::<FavoritesState>()
            .set_is_tracks_loading(is_loading)
    },
};

const ALBUMS: Binding<CardRow> = Binding {
    rows: |window| window.global::<FavoritesState>().get_albums(),
    loading: |window, is_loading| {
        window
            .global::<FavoritesState>()
            .set_is_albums_loading(is_loading)
    },
};

const ARTISTS: Binding<CardRow> = Binding {
    rows: |window| window.global::<FavoritesState>().get_artists(),
    loading: |window, is_loading| {
        window
            .global::<FavoritesState>()
            .set_is_artists_loading(is_loading)
    },
};

pub fn install(window: &AppWindow, context: &Context) {
    let state = window.global::<FavoritesState>();
    state.set_tracks(TRACKS.track_table(window, context, true));
    state.set_albums(card_grid(window, context, ALBUMS));
    state.set_artists(card_grid(window, context, ARTISTS));
    install_load(window, context);
    install_play_track(window, context);
    install_toggles(window, context);
    install_columns(window);
}

fn install_load(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let session = context.session.clone();
    window.global::<FavoritesState>().on_load(move |_| {
        if let Some(window) = weak.upgrade() {
            session.open_view();
            load_favorites(&window);
        }
    });
}

fn load_favorites(window: &AppWindow) {
    let state = window.global::<FavoritesState>();
    state.set_is_tracks_loading(true);
    state.set_is_albums_loading(true);
    state.set_is_artists_loading(true);
    with_table(window, TRACKS, |table| table.reset(favorite_song_query()));
    with_grid(window, ALBUMS, |grid| {
        grid.reset(Some(CardSource::Albums(favorite_album_query())));
    });
    with_grid(window, ARTISTS, |grid| {
        grid.reset(Some(CardSource::Artists(favorite_artist_query())));
    });
}

fn install_play_track(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let play_context = context.clone();
    window
        .global::<FavoritesState>()
        .on_play_track(move |index| {
            if let Some(window) = weak.upgrade() {
                TRACKS.play(&window, &play_context, index);
            }
        });
}

fn install_toggles(window: &AppWindow, context: &Context) {
    let state = window.global::<FavoritesState>();

    let weak = window.as_weak();
    let toggle_context = context.clone();
    state.on_toggle_track(move |index| {
        if let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
            TRACKS.toggle_favorite(&window, &toggle_context, index);
        }
    });

    let weak = window.as_weak();
    let toggle_context = context.clone();
    state.on_toggle_album(move |id| {
        if let Some(window) = weak.upgrade() {
            ALBUMS.toggle_favorite(&window, &toggle_context, &id);
        }
    });

    let weak = window.as_weak();
    let toggle_context = context.clone();
    state.on_toggle_artist(move |id| {
        if let Some(window) = weak.upgrade() {
            ARTISTS.toggle_favorite(&window, &toggle_context, &id);
        }
    });
}

fn install_columns(window: &AppWindow) {
    let state = window.global::<FavoritesState>();

    let weak = window.as_weak();
    state.on_album_columns(move |columns| {
        if let Some(window) = weak.upgrade() {
            set_grid_columns(&window, ALBUMS, columns);
        }
    });

    let weak = window.as_weak();
    state.on_artist_columns(move |columns| {
        if let Some(window) = weak.upgrade() {
            set_grid_columns(&window, ARTISTS, columns);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::controller::Command;
    use crate::demo::FakeServer;
    use crate::list_stream::SongSource;
    use crate::tests::refusing_server::{REFUSAL, RefusingServer};
    use crate::ui::test_window::TestWindow;
    use crate::ui::{AppState, Card, Route};
    use cadence_api::MusicServer;
    use cadence_core::model::Paging;
    use slint::{Model, ModelRc};

    const COLUMNS: i32 = 3;

    fn first_card(rows: &ModelRc<CardRow>) -> Option<Card> {
        rows.row_data(0)?
            .cards
            .row_data(0)
            .filter(|card| card.is_loaded)
    }

    fn open_favorites_on(server: Arc<dyn MusicServer>) -> Option<TestWindow> {
        let view = TestWindow::open(server)?;
        view.app
            .global::<AppState>()
            .invoke_navigate(Route::Favorites, "".into());
        let state = view.app.global::<FavoritesState>();
        state.invoke_album_columns(COLUMNS);
        state.invoke_artist_columns(COLUMNS);
        assert!(view.pump(|| {
            state
                .get_tracks()
                .row_data(0)
                .is_some_and(|item| item.track.is_loaded)
                && first_card(&state.get_albums()).is_some()
                && first_card(&state.get_artists()).is_some()
        }));
        std::iter::from_fn(|| view.next_command()).for_each(drop);
        Some(view)
    }

    fn open_favorites() -> Option<TestWindow> {
        open_favorites_on(Arc::new(FakeServer::new(1_200)))
    }

    #[test]
    fn given_the_favorites_page_when_opened_then_songs_albums_and_artists_are_listed() {
        let Some(view) = open_favorites() else {
            return;
        };
        let state = view.app.global::<FavoritesState>();

        assert!(!state.get_is_tracks_loading());
        assert!(
            state
                .get_tracks()
                .row_data(0)
                .is_some_and(|item| item.is_favorite)
        );
        assert!(first_card(&state.get_albums()).is_some_and(|card| card.is_favorite));
        assert!(first_card(&state.get_artists()).is_some_and(|card| card.is_favorite));
    }

    #[test]
    fn given_a_favorite_song_when_played_then_the_favorites_stream_from_its_row() {
        let Some(view) = open_favorites() else {
            return;
        };

        view.app.global::<FavoritesState>().invoke_play_track(0);

        let played = view.next_command();
        let Some(Command::PlayFromList {
            source: SongSource::Library(query),
            song_id: Some(_),
            ..
        }) = played
        else {
            panic!("expected the favorites to play");
        };
        assert_eq!(query, favorite_song_query());
    }

    #[test]
    fn given_favorites_when_unfavorited_then_their_hearts_empty() {
        let Some(view) = open_favorites() else {
            return;
        };
        let state = view.app.global::<FavoritesState>();
        let album_id = first_card(&state.get_albums()).unwrap().id;
        let artist_id = first_card(&state.get_artists()).unwrap().id;

        state.invoke_toggle_track(0);
        state.invoke_toggle_track(-1);
        state.invoke_toggle_album(album_id);
        state.invoke_toggle_artist(artist_id);

        assert!(
            state
                .get_tracks()
                .row_data(0)
                .is_some_and(|item| !item.is_favorite)
        );
        assert!(first_card(&state.get_albums()).is_some_and(|card| !card.is_favorite));
        assert!(first_card(&state.get_artists()).is_some_and(|card| !card.is_favorite));
    }

    #[test]
    fn given_a_server_refusing_changes_when_unfavoriting_a_song_then_the_heart_comes_back() {
        let Some(view) = open_favorites_on(Arc::new(RefusingServer::new(1_200))) else {
            return;
        };
        let state = view.app.global::<FavoritesState>();

        state.invoke_toggle_track(0);

        assert!(view.pump(|| view.status().starts_with("Could not update favorite")));
        assert_eq!(
            view.status(),
            format!("Could not update favorite: not authorized: {REFUSAL}")
        );
        assert!(
            state
                .get_tracks()
                .row_data(0)
                .is_some_and(|item| item.is_favorite)
        );
    }

    const EVERYTHING: Paging = Paging {
        start_index: 0,
        limit: usize::MAX,
    };

    #[test]
    fn queries_ask_for_favorites_only() {
        assert_eq!(favorite_song_query().is_favorite, Some(true));
        assert_eq!(favorite_album_query().is_favorite, Some(true));
        assert_eq!(favorite_artist_query().is_favorite, Some(true));
    }

    #[tokio::test]
    async fn demo_server_lists_favorites() {
        let server = FakeServer::new(1200);
        let songs = server
            .song_list(&favorite_song_query(), EVERYTHING)
            .await
            .map(|page| page.items)
            .unwrap_or_default();
        assert!(!songs.is_empty() && songs.iter().all(|song| song.is_favorite));
        let albums = server
            .album_list(&favorite_album_query(), EVERYTHING)
            .await
            .map(|page| page.items)
            .unwrap_or_default();
        assert!(!albums.is_empty() && albums.len() < 100);
        assert!(albums.iter().all(|album| album.is_favorite));
        let artists = server
            .artist_list(&favorite_artist_query(), EVERYTHING)
            .await
            .map(|page| page.items)
            .unwrap_or_default();
        assert!(!artists.is_empty() && artists.iter().all(|artist| artist.is_favorite));
    }
}
