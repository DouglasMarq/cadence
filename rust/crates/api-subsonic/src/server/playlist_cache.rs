use std::time::Duration;

use cadence_api::TtlCache;
use cadence_core::model::{Page, Paging, Playlist, Song};

use super::pages::cloned_page;

const PLAYLIST_TIME_TO_LIVE: Duration = Duration::from_secs(30);
const MAX_CACHED_PLAYLISTS: usize = 4;

pub struct CachedPlaylist {
    pub playlist: Playlist,
    songs: Box<[Song]>,
}

impl CachedPlaylist {
    pub fn new(playlist: Playlist, songs: Vec<Song>) -> Self {
        Self {
            playlist,
            songs: songs.into_boxed_slice(),
        }
    }

    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    pub fn song_page(&self, paging: Paging) -> Page<Song> {
        cloned_page(Page::from_all(self.songs.iter(), paging))
    }
}

pub type PlaylistCache = TtlCache<String, CachedPlaylist>;

pub fn playlist_cache() -> PlaylistCache {
    TtlCache::with_max_entries(PLAYLIST_TIME_TO_LIVE, MAX_CACHED_PLAYLISTS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER_ID: &str = "navidrome-example";

    fn song(index: usize) -> Song {
        Song {
            id: format!("s{index}"),
            server_id: SERVER_ID.to_owned(),
            title: format!("Invented Song {index}"),
            artists: Vec::new(),
            album_artists: Vec::new(),
            album: None,
            album_id: None,
            duration_ms: 0,
            track_number: None,
            disc_number: None,
            year: None,
            genres: Vec::new(),
            bitrate_kbps: None,
            play_count: 0,
            user_rating: None,
            is_favorite: false,
            cover_art_id: None,
            replay_gain: None,
            path: None,
            size_bytes: None,
            container: None,
            created_at: None,
            last_played_at: None,
        }
    }

    fn playlist() -> Playlist {
        Playlist {
            id: "p1".to_owned(),
            server_id: SERVER_ID.to_owned(),
            name: "Invented Playlist".to_owned(),
            comment: None,
            owner: None,
            is_public: false,
            song_count: 0,
            duration_ms: 0,
            cover_art_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn cached(count: usize) -> CachedPlaylist {
        CachedPlaylist::new(playlist(), (0..count).map(song).collect())
    }

    #[test]
    fn pages_are_sliced_locally() {
        let playlist = cached(5);
        let page = playlist.song_page(Paging {
            start_index: 3,
            limit: 10,
        });
        assert_eq!(page.total_count, Some(5));
        assert_eq!(page.start_index, 3);
        let ids: Vec<&str> = page.items.iter().map(|song| song.id.as_str()).collect();
        assert_eq!(ids, ["s3", "s4"]);
        let past_end = playlist.song_page(Paging {
            start_index: 9,
            limit: 2,
        });
        assert!(past_end.items.is_empty());
    }
}
