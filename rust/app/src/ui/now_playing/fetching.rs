use std::rc::Rc;
use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::Song;
use cadence_storage::settings::LyricsSourceSetting;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::loading::{DetailsRequest, is_current};
use super::lyrics_plan::lyrics_plan;
use super::song_lyrics::{LyricsLookup, LyricsLookupError, SongLyrics};
use super::view::{View, with_view};
use crate::session::Session;
use crate::ui::images::{CoverSource, to_image};
use crate::ui::{AppWindow, Context, LyricsStatus, NowPlayingState, TrackRow, on_ui, tracks};

const COVER_SIZE: u32 = 600;
const SIMILAR_COUNT: usize = 20;

pub(super) struct SongCover {
    pub(super) song_id: String,
    pub(super) source: CoverSource,
}

pub(super) async fn fetch_cover(request: &DetailsRequest, song: &Song) {
    let Some(cover_id) = song.cover_art_id.clone() else {
        return;
    };
    let source = CoverSource::Server {
        server: Arc::clone(&request.server),
        cover_id,
    };
    let cover = SongCover {
        song_id: song.id.clone(),
        source,
    };
    show_cover(&request.weak_window, &request.context, cover).await;
}

pub(super) async fn show_cover(
    weak_window: &slint::Weak<AppWindow>,
    context: &Context,
    cover: SongCover,
) {
    let SongCover { song_id, source } = cover;
    let Some(cover) = source.fetch(context.covers.as_ref(), COVER_SIZE).await else {
        return;
    };
    on_ui(weak_window, move |window| {
        if is_current(window, &song_id)
            && let Some(image) = to_image(&cover)
        {
            let state = window.global::<NowPlayingState>();
            state.set_cover(image);
            state.set_has_cover(true);
        }
    });
}

pub(super) async fn fetch_similar(
    weak_window: &slint::Weak<AppWindow>,
    server: &dyn MusicServer,
    song_id: &str,
) {
    let songs = server
        .similar_songs(song_id, SIMILAR_COUNT)
        .await
        .unwrap_or_else(|error| {
            tracing::debug!("similar songs for {song_id}: {error}");
            Vec::new()
        });
    let song_id = song_id.to_owned();
    on_ui(weak_window, move |window| {
        if !is_current(window, &song_id) {
            return;
        }
        let rows: Vec<TrackRow> = songs
            .iter()
            .enumerate()
            .map(|(index, song)| tracks::track_row(song, index))
            .collect();
        let state = window.global::<NowPlayingState>();
        state.set_similar(ModelRc::new(VecModel::from(rows)));
        state.set_is_similar_loading(false);
        with_view(|view| *view.similar.borrow_mut() = songs);
    });
}

pub(super) fn configured_lyrics_sources(session: &Session) -> Vec<LyricsSourceSetting> {
    session.settings().integrations.lyrics_sources.clone()
}

pub(super) async fn fetch_lyrics(request: &DetailsRequest, song: &Song) {
    let configured = configured_lyrics_sources(&request.context.session);
    let provider = request.context.connector.lyrics();
    let plan = lyrics_plan(&configured);
    let lookup = LyricsLookup {
        server: request.server.as_ref(),
        provider: provider.as_ref(),
    };
    let result = lookup.resolve(song, &plan).await;
    let song_id = song.id.clone();
    on_ui(&request.weak_window, move |window| {
        with_view(|view| view.apply_lyrics_result(window, &song_id, result));
    });
}

impl View {
    pub(super) fn apply_lyrics_result(
        &self,
        window: &AppWindow,
        song_id: &str,
        result: Result<Option<SongLyrics>, LyricsLookupError>,
    ) {
        let lyrics = match result {
            Ok(lyrics) => lyrics.map(Rc::new),
            Err(error) => {
                tracing::warn!("lyrics for {song_id}: {error}");
                if is_current(window, song_id) {
                    window
                        .global::<NowPlayingState>()
                        .set_lyrics_status(LyricsStatus::Failed);
                }
                return;
            }
        };
        self.remember(song_id.to_owned(), lyrics.clone());
        if is_current(window, song_id) {
            self.show_lyrics(window, lyrics.as_deref());
        }
    }
}
