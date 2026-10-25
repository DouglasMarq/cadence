use async_trait::async_trait;
use cadence_api::{Credential, MusicServer, Result, StreamOptions};
use cadence_core::model::{
    Album, AlbumDetail, AlbumListQuery, Artist, ArtistDetail, ArtistListQuery, Folder, FolderId,
    Genre, ItemKind, LibraryScan, MusicFolder, Page, Paging, Playlist, RadioStation,
    RadioStationDraft, SearchResults, ServerConfig, ServerPlayQueue, Share, Song, SongListQuery,
};

use crate::NavidromeServer;

#[async_trait]
impl MusicServer for NavidromeServer {
    fn config(&self) -> &ServerConfig {
        &self.config
    }

    fn credential(&self) -> Option<Credential> {
        Some(NavidromeServer::credential(self))
    }

    async fn ping(&self) -> Result<()> {
        self.subsonic_server.ping().await
    }

    async fn song_list(&self, query: &SongListQuery, paging: Paging) -> Result<Page<Song>> {
        self.list_songs(query, paging).await
    }

    async fn song_count(&self, query: &SongListQuery) -> Result<usize> {
        self.count_songs(query).await
    }

    async fn song(&self, id: &str) -> Result<Song> {
        self.fetch_song(id).await
    }

    async fn random_songs(&self, count: usize, genre_id: Option<&str>) -> Result<Vec<Song>> {
        self.fetch_random_songs(count, genre_id).await
    }

    async fn similar_songs(&self, song_id: &str, count: usize) -> Result<Vec<Song>> {
        self.subsonic_server.similar_songs(song_id, count).await
    }

    async fn album_list(&self, query: &AlbumListQuery, paging: Paging) -> Result<Page<Album>> {
        self.list_albums(query, paging).await
    }

    async fn album_count(&self, query: &AlbumListQuery) -> Result<usize> {
        self.count_albums(query).await
    }

    async fn album(&self, id: &str) -> Result<AlbumDetail> {
        self.fetch_album_detail(id).await
    }

    async fn artist_list(&self, query: &ArtistListQuery, paging: Paging) -> Result<Page<Artist>> {
        self.list_artists(query, paging).await
    }

    async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        self.fetch_artist_detail(id).await
    }

    async fn genre_list(&self) -> Result<Vec<Genre>> {
        self.list_genres().await
    }

    async fn music_folders(&self) -> Result<Vec<MusicFolder>> {
        self.subsonic_server.music_folders().await
    }

    async fn playlist_list(&self, paging: Paging) -> Result<Page<Playlist>> {
        self.list_playlists(paging).await
    }

    async fn playlist_songs(&self, id: &str) -> Result<Vec<Song>> {
        self.fetch_playlist_songs(id).await
    }

    async fn playlist(&self, id: &str) -> Result<Playlist> {
        self.fetch_playlist(id).await
    }

    async fn playlist_song_page(&self, id: &str, paging: Paging) -> Result<Page<Song>> {
        self.fetch_playlist_song_page(id, paging).await
    }

    async fn create_playlist(&self, name: &str, song_ids: &[String]) -> Result<Playlist> {
        self.create_playlist_with_songs(name, song_ids).await
    }

    async fn add_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        self.add_songs_to_playlist(playlist_id, song_ids).await
    }

    async fn remove_from_playlist(&self, playlist_id: &str, indexes: &[usize]) -> Result<()> {
        self.remove_playlist_entries(playlist_id, indexes).await
    }

    async fn delete_playlist(&self, id: &str) -> Result<()> {
        self.remove_playlist(id).await
    }

    async fn search(&self, query: &str, limit: usize) -> Result<SearchResults> {
        self.subsonic_server.search(query, limit).await
    }

    async fn set_favorite(&self, kind: ItemKind, ids: &[String], is_favorite: bool) -> Result<()> {
        self.subsonic_server
            .set_favorite(kind, ids, is_favorite)
            .await
    }

    async fn set_rating(&self, kind: ItemKind, id: &str, rating: u8) -> Result<()> {
        self.subsonic_server.set_rating(kind, id, rating).await
    }

    async fn scrobble(
        &self,
        song_id: &str,
        is_submission: bool,
        timestamp_ms: Option<i64>,
    ) -> Result<()> {
        self.subsonic_server
            .scrobble(song_id, is_submission, timestamp_ms)
            .await
    }

    async fn save_play_queue(
        &self,
        song_ids: &[String],
        current_song_id: Option<&str>,
        position_ms: u64,
    ) -> Result<()> {
        self.store_play_queue(song_ids, current_song_id, position_ms)
            .await
    }

    async fn play_queue(&self) -> Result<Option<ServerPlayQueue>> {
        self.fetch_play_queue().await
    }

    async fn lyrics(&self, song: &Song) -> Result<Option<String>> {
        self.subsonic_server.lyrics(song).await
    }

    async fn start_library_scan(&self) -> Result<LibraryScan> {
        self.subsonic_server.start_library_scan().await
    }

    async fn library_scan(&self) -> Result<LibraryScan> {
        self.subsonic_server.library_scan().await
    }

    async fn subfolders(&self, folder: &FolderId) -> Result<Vec<Folder>> {
        self.subsonic_server.subfolders(folder).await
    }

    async fn folder_song_page(&self, folder: &FolderId, paging: Paging) -> Result<Page<Song>> {
        self.subsonic_server.folder_song_page(folder, paging).await
    }

    fn can_share(&self) -> bool {
        true
    }

    async fn create_share(&self, item_ids: &[String], description: Option<&str>) -> Result<Share> {
        self.share_items(item_ids, description).await
    }

    async fn radio_stations(&self) -> Result<Vec<RadioStation>> {
        self.subsonic_server.radio_stations().await
    }

    async fn can_manage_radio_stations(&self) -> Result<bool> {
        self.subsonic_server.can_manage_radio_stations().await
    }

    async fn create_radio_station(&self, station: &RadioStationDraft) -> Result<()> {
        self.subsonic_server.create_radio_station(station).await
    }

    async fn update_radio_station(&self, id: &str, station: &RadioStationDraft) -> Result<()> {
        self.subsonic_server.update_radio_station(id, station).await
    }

    async fn delete_radio_station(&self, id: &str) -> Result<()> {
        self.subsonic_server.delete_radio_station(id).await
    }

    fn stream_url(&self, song_id: &str, options: &StreamOptions) -> String {
        self.build_stream_url(song_id, options)
    }

    fn cover_art_url(&self, cover_art_id: &str, size: Option<u32>) -> String {
        self.build_cover_art_url(cover_art_id, size)
    }
}
