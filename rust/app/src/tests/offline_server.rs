use async_trait::async_trait;
use cadence_api::{ApiError, MusicServer, Result, StreamOptions};
use cadence_core::model::{
    Album, AlbumDetail, AlbumListQuery, Artist, ArtistDetail, ArtistListQuery, Folder, FolderId,
    Genre, ItemKind, LibraryScan, MusicFolder, Page, Paging, Playlist, RadioStation,
    RadioStationDraft, SearchResults, ServerConfig, ServerPlayQueue, Share, Song, SongListQuery,
};

use crate::demo::FakeServer;

pub const OFFLINE: &str = "network error: the server is offline";

pub struct OfflineServer {
    config: ServerConfig,
}

impl Default for OfflineServer {
    fn default() -> Self {
        Self {
            config: FakeServer::config_for_demo(),
        }
    }
}

fn offline<Value>() -> Result<Value> {
    Err(ApiError::Network("the server is offline".into()))
}

#[async_trait]
impl MusicServer for OfflineServer {
    fn config(&self) -> &ServerConfig {
        &self.config
    }

    async fn ping(&self) -> Result<()> {
        offline()
    }

    async fn song_list(&self, _query: &SongListQuery, _paging: Paging) -> Result<Page<Song>> {
        offline()
    }

    async fn song_count(&self, _query: &SongListQuery) -> Result<usize> {
        offline()
    }

    async fn song(&self, _id: &str) -> Result<Song> {
        offline()
    }

    async fn random_songs(&self, _count: usize, _genre: Option<&str>) -> Result<Vec<Song>> {
        offline()
    }

    async fn similar_songs(&self, _song_id: &str, _count: usize) -> Result<Vec<Song>> {
        offline()
    }

    async fn album_list(&self, _query: &AlbumListQuery, _paging: Paging) -> Result<Page<Album>> {
        offline()
    }

    async fn album_count(&self, _query: &AlbumListQuery) -> Result<usize> {
        offline()
    }

    async fn album(&self, _id: &str) -> Result<AlbumDetail> {
        offline()
    }

    async fn artist_list(&self, _query: &ArtistListQuery, _paging: Paging) -> Result<Page<Artist>> {
        offline()
    }

    async fn artist(&self, _id: &str) -> Result<ArtistDetail> {
        offline()
    }

    async fn genre_list(&self) -> Result<Vec<Genre>> {
        offline()
    }

    async fn music_folders(&self) -> Result<Vec<MusicFolder>> {
        offline()
    }

    async fn playlist_list(&self, _paging: Paging) -> Result<Page<Playlist>> {
        offline()
    }

    async fn playlist_songs(&self, _id: &str) -> Result<Vec<Song>> {
        offline()
    }

    async fn playlist(&self, _id: &str) -> Result<Playlist> {
        offline()
    }

    async fn playlist_song_page(&self, _id: &str, _paging: Paging) -> Result<Page<Song>> {
        offline()
    }

    async fn create_playlist(&self, _name: &str, _song_ids: &[String]) -> Result<Playlist> {
        offline()
    }

    async fn add_to_playlist(&self, _playlist_id: &str, _song_ids: &[String]) -> Result<()> {
        offline()
    }

    async fn remove_from_playlist(&self, _playlist_id: &str, _indexes: &[usize]) -> Result<()> {
        offline()
    }

    async fn delete_playlist(&self, _id: &str) -> Result<()> {
        offline()
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<SearchResults> {
        offline()
    }

    async fn set_favorite(
        &self,
        _kind: ItemKind,
        _ids: &[String],
        _is_favorite: bool,
    ) -> Result<()> {
        offline()
    }

    async fn set_rating(&self, _kind: ItemKind, _id: &str, _rating: u8) -> Result<()> {
        offline()
    }

    async fn scrobble(
        &self,
        _song_id: &str,
        _is_submission: bool,
        _timestamp_ms: Option<i64>,
    ) -> Result<()> {
        offline()
    }

    async fn save_play_queue(
        &self,
        _song_ids: &[String],
        _current_song_id: Option<&str>,
        _position_ms: u64,
    ) -> Result<()> {
        offline()
    }

    async fn play_queue(&self) -> Result<Option<ServerPlayQueue>> {
        offline()
    }

    async fn lyrics(&self, _song: &Song) -> Result<Option<String>> {
        offline()
    }

    async fn start_library_scan(&self) -> Result<LibraryScan> {
        offline()
    }

    async fn library_scan(&self) -> Result<LibraryScan> {
        offline()
    }

    async fn subfolders(&self, _folder: &FolderId) -> Result<Vec<Folder>> {
        offline()
    }

    async fn folder_song_page(&self, _folder: &FolderId, _paging: Paging) -> Result<Page<Song>> {
        offline()
    }

    fn can_share(&self) -> bool {
        false
    }

    async fn create_share(
        &self,
        _item_ids: &[String],
        _description: Option<&str>,
    ) -> Result<Share> {
        offline()
    }

    async fn radio_stations(&self) -> Result<Vec<RadioStation>> {
        offline()
    }

    async fn can_manage_radio_stations(&self) -> Result<bool> {
        offline()
    }

    async fn create_radio_station(&self, _station: &RadioStationDraft) -> Result<()> {
        offline()
    }

    async fn update_radio_station(&self, _id: &str, _station: &RadioStationDraft) -> Result<()> {
        offline()
    }

    async fn delete_radio_station(&self, _id: &str) -> Result<()> {
        offline()
    }

    fn stream_url(&self, song_id: &str, _options: &StreamOptions) -> String {
        format!("offline://stream/{song_id}")
    }

    fn cover_art_url(&self, cover_art_id: &str, _size: Option<u32>) -> String {
        format!("offline://cover/{cover_art_id}")
    }
}
