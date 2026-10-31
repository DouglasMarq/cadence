use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use cadence_api::{MusicServer, Result, StreamOptions};
use cadence_core::model::{
    Album, AlbumDetail, AlbumListQuery, Artist, ArtistDetail, ArtistListQuery, Folder, FolderId,
    Genre, ItemKind, LibraryScan, MusicFolder, Page, Paging, Playlist, RadioStation,
    RadioStationDraft, SearchResults, ServerConfig, ServerPlayQueue, Share, Song, SongListQuery,
};
use tokio::sync::watch;

pub struct HeldServer {
    library: Arc<dyn MusicServer>,
    opened: watch::Sender<bool>,
    answers: AtomicUsize,
}

impl HeldServer {
    pub fn new(library: Arc<dyn MusicServer>) -> Self {
        Self {
            library,
            opened: watch::Sender::new(false),
            answers: AtomicUsize::new(0),
        }
    }

    pub fn release(&self) {
        self.opened.send_replace(true);
    }

    pub fn answers(&self) -> usize {
        self.answers.load(Ordering::SeqCst)
    }

    async fn wait_for_release(&self) {
        let mut opened = self.opened.subscribe();
        let released = opened.wait_for(|is_open| *is_open).await;
        assert!(released.is_ok(), "the held server owns its sender");
        self.answers.fetch_add(1, Ordering::SeqCst);
    }
}

macro_rules! held_calls {
    ($($call:ident($($argument:ident: $kind:ty),*) -> $output:ty;)*) => {
        #[async_trait]
        impl MusicServer for HeldServer {
            fn config(&self) -> &ServerConfig {
                self.library.config()
            }

            fn can_share(&self) -> bool {
                self.library.can_share()
            }

            fn stream_url(&self, song_id: &str, options: &StreamOptions) -> String {
                self.library.stream_url(song_id, options)
            }

            fn cover_art_url(&self, cover_art_id: &str, edge_pixels: Option<u32>) -> String {
                self.library.cover_art_url(cover_art_id, edge_pixels)
            }

            $(async fn $call(&self, $($argument: $kind),*) -> Result<$output> {
                self.wait_for_release().await;
                self.library.$call($($argument),*).await
            })*
        }
    };
}

held_calls! {
    ping() -> ();
    song_list(query: &SongListQuery, paging: Paging) -> Page<Song>;
    song_count(query: &SongListQuery) -> usize;
    song(id: &str) -> Song;
    random_songs(count: usize, genre_id: Option<&str>) -> Vec<Song>;
    similar_songs(song_id: &str, count: usize) -> Vec<Song>;
    album_list(query: &AlbumListQuery, paging: Paging) -> Page<Album>;
    album_count(query: &AlbumListQuery) -> usize;
    album(id: &str) -> AlbumDetail;
    artist_list(query: &ArtistListQuery, paging: Paging) -> Page<Artist>;
    artist(id: &str) -> ArtistDetail;
    genre_list() -> Vec<Genre>;
    music_folders() -> Vec<MusicFolder>;
    playlist_list(paging: Paging) -> Page<Playlist>;
    playlist_songs(id: &str) -> Vec<Song>;
    playlist(id: &str) -> Playlist;
    playlist_song_page(id: &str, paging: Paging) -> Page<Song>;
    create_playlist(name: &str, song_ids: &[String]) -> Playlist;
    add_to_playlist(playlist_id: &str, song_ids: &[String]) -> ();
    remove_from_playlist(playlist_id: &str, playlist_positions: &[usize]) -> ();
    delete_playlist(id: &str) -> ();
    search(query: &str, limit: usize) -> SearchResults;
    set_favorite(kind: ItemKind, ids: &[String], is_favorite: bool) -> ();
    set_rating(kind: ItemKind, id: &str, rating_zero_clears: u8) -> ();
    scrobble(song_id: &str, is_submission: bool, timestamp_ms: Option<i64>) -> ();
    save_play_queue(song_ids: &[String], current_song_id: Option<&str>, position_ms: u64) -> ();
    play_queue() -> Option<ServerPlayQueue>;
    lyrics(song: &Song) -> Option<String>;
    start_library_scan() -> LibraryScan;
    library_scan() -> LibraryScan;
    subfolders(folder: &FolderId) -> Vec<Folder>;
    folder_song_page(folder: &FolderId, paging: Paging) -> Page<Song>;
    create_share(item_ids: &[String], description: Option<&str>) -> Share;
    radio_stations() -> Vec<RadioStation>;
    can_manage_radio_stations() -> bool;
    create_radio_station(station: &RadioStationDraft) -> ();
    update_radio_station(id: &str, station: &RadioStationDraft) -> ();
    delete_radio_station(id: &str) -> ();
}
