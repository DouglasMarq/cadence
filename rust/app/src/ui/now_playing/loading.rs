use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::Song;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::fetching::{fetch_cover, fetch_lyrics, fetch_similar};
use super::view::View;
use crate::ui::{AppWindow, ArtistLink, Context, LyricsStatus, NowPlayingState, on_ui};

pub(super) fn is_current(window: &AppWindow, song_id: &str) -> bool {
    window.global::<NowPlayingState>().get_song_id() == song_id
}

fn is_already_shown(state: &NowPlayingState, song_id: &str) -> bool {
    state.get_song_id() == song_id && (state.get_is_loaded() || song_id.is_empty())
}

impl View {
    pub(super) fn load(&self, window: &AppWindow, context: &Context, song_id: &str) {
        self.refresh_up_next(window);
        let state = window.global::<NowPlayingState>();
        if is_already_shown(&state, song_id) {
            return;
        }
        self.clear(window, song_id);
        if let Some(track) = self.external_track(song_id) {
            self.load_external(window, context, &track);
            return;
        }
        let Some(server) = context.session.get().filter(|_| !song_id.is_empty()) else {
            return;
        };

        let cached = self.cached_lyrics(song_id);
        match &cached {
            Some(lyrics) => self.show_lyrics(window, lyrics.as_deref()),
            None => state.set_lyrics_status(LyricsStatus::Loading),
        }
        state.set_is_similar_loading(true);

        let request = DetailsRequest {
            weak_window: window.as_weak(),
            context: context.clone(),
            server,
            song_id: song_id.to_owned(),
            is_lyrics_wanted: cached.is_none(),
        };
        request.spawn();
    }
}

pub(super) struct DetailsRequest {
    pub(super) weak_window: slint::Weak<AppWindow>,
    pub(super) context: Context,
    pub(super) server: Arc<dyn MusicServer>,
    song_id: String,
    is_lyrics_wanted: bool,
}

impl DetailsRequest {
    fn spawn(self) {
        let runtime = self.context.runtime.clone();
        runtime.spawn(self.run());
    }

    async fn run(self) {
        let song = match self.server.song(&self.song_id).await {
            Ok(song) => song,
            Err(error) => {
                tracing::warn!("now playing {}: {error}", self.song_id);
                show_details_failed(&self.weak_window, self.song_id);
                return;
            }
        };
        show_song(&self.weak_window, &song);
        tokio::join!(
            fetch_cover(&self, &song),
            fetch_similar(&self.weak_window, self.server.as_ref(), &self.song_id),
            async {
                if self.is_lyrics_wanted {
                    fetch_lyrics(&self, &song).await;
                }
            },
        );
    }
}

fn show_details_failed(weak_window: &slint::Weak<AppWindow>, song_id: String) {
    on_ui(weak_window, move |window| {
        if !is_current(window, &song_id) {
            return;
        }
        let state = window.global::<NowPlayingState>();
        state.set_is_similar_loading(false);
        if state.get_lyrics_status() == LyricsStatus::Loading {
            state.set_lyrics_status(LyricsStatus::Failed);
        }
    });
}

fn artist_links(song: &Song) -> Vec<ArtistLink> {
    song.artists
        .iter()
        .map(|artist| ArtistLink {
            id: artist.id.as_str().into(),
            name: artist.name.as_str().into(),
        })
        .collect()
}

fn show_song(weak_window: &slint::Weak<AppWindow>, song: &Song) {
    let song_id = song.id.clone();
    let artists = artist_links(song);
    let album_id = song.album_id.clone().unwrap_or_default();
    on_ui(weak_window, move |window| {
        if !is_current(window, &song_id) {
            return;
        }
        let state = window.global::<NowPlayingState>();
        state.set_artists(ModelRc::new(VecModel::from(artists)));
        state.set_album_id(album_id.into());
        state.set_is_loaded(true);
    });
}
