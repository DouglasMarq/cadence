use cadence_core::model::{Playlist, Song};

use crate::ui::playlists::song_count_label;
use crate::ui::tracks::{placeholder_track, track_row};
use crate::ui::{PlaylistSongRow, format_duration};

pub struct HeaderSummary<'playlist> {
    pub song_count: usize,
    pub duration_ms: Option<u64>,
    pub owner: Option<&'playlist str>,
}

impl<'playlist> HeaderSummary<'playlist> {
    pub fn new(metadata: Option<&'playlist Playlist>, loaded_total: usize) -> Self {
        Self {
            song_count: metadata
                .map(|playlist| playlist.song_count as usize)
                .filter(|count| *count > 0)
                .unwrap_or(loaded_total),
            duration_ms: metadata.map(|playlist| playlist.duration_ms),
            owner: metadata.and_then(|playlist| playlist.owner.as_deref()),
        }
    }
}

pub fn header_info(summary: &HeaderSummary) -> String {
    let mut parts = vec![song_count_label(summary.song_count)];
    parts.extend(
        summary
            .duration_ms
            .filter(|duration| *duration > 0 && summary.song_count > 0)
            .map(format_duration),
    );
    parts.extend(
        summary
            .owner
            .filter(|owner| !owner.is_empty())
            .map(|owner| tr!("by {}", owner)),
    );
    parts.join(" • ")
}

pub fn song_row(song: &Song, index: usize, is_selected: bool) -> PlaylistSongRow {
    let track = track_row(song, index);
    PlaylistSongRow {
        album_id: track.album_id.clone(),
        artist_id: track.artist_id.clone(),
        track,
        is_selected,
    }
}

pub fn placeholder_row(index: usize, is_selected: bool) -> PlaylistSongRow {
    PlaylistSongRow {
        track: placeholder_track(index),
        is_selected,
        ..PlaylistSongRow::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadence_core::model::ArtistRef;

    use crate::demo::{FakeServer, test_song};

    fn summary(song_count: usize, duration_ms: Option<u64>) -> HeaderSummary<'static> {
        HeaderSummary {
            song_count,
            duration_ms,
            owner: Some("demo"),
        }
    }

    #[test]
    fn header_info_reads_the_playlist_totals() {
        assert_eq!(
            header_info(&summary(2, Some(150_000))),
            "2 songs • 2:30 • by demo"
        );
        assert_eq!(header_info(&summary(3, None)), "3 songs • by demo");
        let empty = HeaderSummary {
            song_count: 0,
            duration_ms: Some(0),
            owner: None,
        };
        assert_eq!(header_info(&empty), "0 songs");
    }

    #[test]
    fn header_summary_prefers_the_playlist_metadata() {
        let playlist = Playlist {
            id: "playlist-1".into(),
            server_id: "demo".into(),
            name: "Mix".into(),
            comment: None,
            owner: Some("demo".into()),
            is_public: false,
            song_count: 12,
            duration_ms: 600_000,
            cover_art_id: None,
            created_at: None,
            updated_at: None,
        };
        let summary = HeaderSummary::new(Some(&playlist), 999);
        assert_eq!(summary.song_count, 12);
        assert_eq!(summary.duration_ms, Some(600_000));
        assert_eq!(summary.owner, Some("demo"));
        assert_eq!(HeaderSummary::new(None, 7).song_count, 7);
    }

    #[test]
    fn song_rows_link_album_and_artist() {
        let server = FakeServer::new(50);
        let row = song_row(&server.song_at(13), 0, true);
        assert_eq!(row.album_id, "album-1");
        assert!(row.artist_id.starts_with("artist-"));
        assert_eq!(row.track.index, 1);
        assert!(row.is_selected);
        let placeholder = placeholder_row(4, false);
        assert_eq!(placeholder.track.index, 5);
        assert!(!placeholder.track.is_loaded);
    }

    #[test]
    fn given_a_song_with_only_album_artists_when_its_row_is_built_then_it_links_the_album_artist() {
        let song = Song {
            album_artists: vec![ArtistRef {
                id: "album-artist-1".to_owned(),
                name: "Album Artist".to_owned(),
            }],
            ..test_song("song-1", 1_000)
        };
        assert_eq!(song_row(&song, 0, false).artist_id, "album-artist-1");
    }
}
