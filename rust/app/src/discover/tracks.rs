use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::Song;
use cadence_core::queue::QueueTrack;
use cadence_youtube::music::MusicSong;

use super::library_match::LibraryMatcher;
use crate::youtube::youtube_track_of;

pub fn queue_track_for(song: &MusicSong, library_song: Option<&Song>) -> QueueTrack {
    match library_song {
        Some(library_song) => QueueTrack::from(library_song),
        None => QueueTrack::from(&youtube_track_of(song)),
    }
}

pub async fn resolve_tracks(
    matcher: &LibraryMatcher,
    server: Option<Arc<dyn MusicServer>>,
    songs: &[MusicSong],
) -> Vec<QueueTrack> {
    let matches = match server {
        Some(server) => matcher.matches(server, songs).await,
        None => vec![None; songs.len()],
    };
    songs
        .iter()
        .zip(matches)
        .map(|(song, matched)| queue_track_for(song, matched.as_ref()))
        .collect()
}
