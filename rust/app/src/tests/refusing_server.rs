use async_trait::async_trait;
use cadence_api::{ApiError, MusicServer, Result, StreamOptions};
use cadence_core::model::{
    Album, AlbumDetail, AlbumListQuery, Artist, ArtistDetail, ArtistListQuery, Folder, FolderId,
    Genre, ItemKind, LibraryScan, MusicFolder, Page, Paging, Playlist, RadioStation,
    RadioStationDraft, SearchResults, ServerConfig, ServerPlayQueue, Share, Song, SongListQuery,
};

use crate::demo::FakeServer;

pub(crate) const REFUSAL: &str = "read-only demo";

pub(crate) struct RefusingServer {
    library: FakeServer,
}

impl RefusingServer {
    pub(crate) fn new(song_count: usize) -> Self {
        Self {
            library: FakeServer::new(song_count),
        }
    }
}

fn refused<Output>() -> Result<Output> {
    Err(ApiError::NotAuthorized(REFUSAL.into()))
}

#[async_trait]
impl MusicServer for RefusingServer {
    fn config(&self) -> &ServerConfig {
        self.library.config()
    }

    async fn ping(&self) -> Result<()> {
        self.library.ping().await
    }

    async fn song_list(&self, query: &SongListQuery, paging: Paging) -> Result<Page<Song>> {
        self.library.song_list(query, paging).await
    }

    async fn song_count(&self, query: &SongListQuery) -> Result<usize> {
        self.library.song_count(query).await
    }

    async fn song(&self, id: &str) -> Result<Song> {
        self.library.song(id).await
    }

    async fn random_songs(&self, count: usize, genre_id: Option<&str>) -> Result<Vec<Song>> {
        self.library.random_songs(count, genre_id).await
    }

    async fn similar_songs(&self, song_id: &str, count: usize) -> Result<Vec<Song>> {
        self.library.similar_songs(song_id, count).await
    }

    async fn album_list(&self, query: &AlbumListQuery, paging: Paging) -> Result<Page<Album>> {
        self.library.album_list(query, paging).await
    }

    async fn album_count(&self, query: &AlbumListQuery) -> Result<usize> {
        self.library.album_count(query).await
    }

    async fn album(&self, id: &str) -> Result<AlbumDetail> {
        self.library.album(id).await
    }

    async fn artist_list(&self, query: &ArtistListQuery, paging: Paging) -> Result<Page<Artist>> {
        self.library.artist_list(query, paging).await
    }

    async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        self.library.artist(id).await
    }

    async fn genre_list(&self) -> Result<Vec<Genre>> {
        self.library.genre_list().await
    }

    async fn music_folders(&self) -> Result<Vec<MusicFolder>> {
        self.library.music_folders().await
    }

    async fn playlist_list(&self, paging: Paging) -> Result<Page<Playlist>> {
        self.library.playlist_list(paging).await
    }

    async fn playlist_songs(&self, id: &str) -> Result<Vec<Song>> {
        self.library.playlist_songs(id).await
    }

    async fn playlist(&self, id: &str) -> Result<Playlist> {
        self.library.playlist(id).await
    }

    async fn playlist_song_page(&self, id: &str, paging: Paging) -> Result<Page<Song>> {
        self.library.playlist_song_page(id, paging).await
    }

    async fn create_playlist(&self, _name: &str, _song_ids: &[String]) -> Result<Playlist> {
        refused()
    }

    async fn add_to_playlist(&self, _playlist_id: &str, _song_ids: &[String]) -> Result<()> {
        refused()
    }

    async fn remove_from_playlist(
        &self,
        _playlist_id: &str,
        _playlist_positions: &[usize],
    ) -> Result<()> {
        refused()
    }

    async fn delete_playlist(&self, _id: &str) -> Result<()> {
        refused()
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        self.library.search(query, limit).await
    }

    async fn set_favorite(
        &self,
        _kind: ItemKind,
        _ids: &[String],
        _is_favorite: bool,
    ) -> Result<()> {
        refused()
    }

    async fn set_rating(&self, _kind: ItemKind, _id: &str, _rating_zero_clears: u8) -> Result<()> {
        refused()
    }

    async fn scrobble(
        &self,
        _song_id: &str,
        _is_submission: bool,
        _timestamp_ms: Option<i64>,
    ) -> Result<()> {
        refused()
    }

    async fn save_play_queue(
        &self,
        _song_ids: &[String],
        _current_song_id: Option<&str>,
        _position_ms: u64,
    ) -> Result<()> {
        refused()
    }

    async fn play_queue(&self) -> Result<Option<ServerPlayQueue>> {
        self.library.play_queue().await
    }

    async fn lyrics(&self, song: &Song) -> Result<Option<String>> {
        self.library.lyrics(song).await
    }

    async fn start_library_scan(&self) -> Result<LibraryScan> {
        refused()
    }

    async fn library_scan(&self) -> Result<LibraryScan> {
        self.library.library_scan().await
    }

    async fn subfolders(&self, folder: &FolderId) -> Result<Vec<Folder>> {
        self.library.subfolders(folder).await
    }

    async fn folder_song_page(&self, folder: &FolderId, paging: Paging) -> Result<Page<Song>> {
        self.library.folder_song_page(folder, paging).await
    }

    fn can_share(&self) -> bool {
        self.library.can_share()
    }

    async fn create_share(
        &self,
        _item_ids: &[String],
        _description: Option<&str>,
    ) -> Result<Share> {
        refused()
    }

    async fn radio_stations(&self) -> Result<Vec<RadioStation>> {
        self.library.radio_stations().await
    }

    async fn can_manage_radio_stations(&self) -> Result<bool> {
        Ok(false)
    }

    async fn create_radio_station(&self, _station: &RadioStationDraft) -> Result<()> {
        refused()
    }

    async fn update_radio_station(&self, _id: &str, _station: &RadioStationDraft) -> Result<()> {
        refused()
    }

    async fn delete_radio_station(&self, _id: &str) -> Result<()> {
        refused()
    }

    fn stream_url(&self, song_id: &str, options: &StreamOptions) -> String {
        self.library.stream_url(song_id, options)
    }

    fn cover_art_url(&self, cover_art_id: &str, edge_pixels: Option<u32>) -> String {
        self.library.cover_art_url(cover_art_id, edge_pixels)
    }
}
