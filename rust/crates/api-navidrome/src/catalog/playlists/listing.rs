use cadence_api::{MusicServer, Result};
use cadence_core::model::{Page, Paging, Playlist};

use super::covers::with_cache_busted_cover;
use crate::NavidromeServer;
use crate::native::{NativePlaylist, normalize_playlist};
use crate::query::{QueryParams, end_index};

impl NavidromeServer {
    pub(crate) async fn list_playlists(&self, paging: Paging) -> Result<Page<Playlist>> {
        self.native_or_subsonic(
            self.native_playlist_page(paging),
            self.subsonic_playlist_page(paging),
        )
        .await
    }

    pub(crate) async fn fetch_playlist(&self, id: &str) -> Result<Playlist> {
        self.native_or_subsonic(self.native_playlist(id), self.subsonic_playlist(id))
            .await
    }

    pub(crate) async fn native_playlist(&self, id: &str) -> Result<Playlist> {
        let (playlist, _): (NativePlaylist, _) = self.native_get(&["playlist", id], &[]).await?;
        Ok(normalize_playlist(playlist, self.server_id()))
    }

    async fn subsonic_playlist(&self, id: &str) -> Result<Playlist> {
        let playlist = self.subsonic_server.playlist(id).await?;
        Ok(with_cache_busted_cover(playlist))
    }

    async fn native_playlist_page(&self, paging: Paging) -> Result<Page<Playlist>> {
        let start = paging.start_index;
        let params: QueryParams = vec![
            ("_start", start.to_string()),
            ("_end", end_index(start, paging.limit).to_string()),
            ("_sort", "name".into()),
            ("_order", "ASC".into()),
        ];
        let (playlists, total): (Vec<NativePlaylist>, _) =
            self.native_get(&["playlist"], &params).await?;
        Ok(Page {
            items: playlists
                .into_iter()
                .map(|playlist| normalize_playlist(playlist, self.server_id()))
                .collect(),
            start_index: start,
            total_count: total,
        })
    }

    async fn subsonic_playlist_page(&self, paging: Paging) -> Result<Page<Playlist>> {
        let page = self.subsonic_server.playlist_list(paging).await?;
        Ok(Page {
            items: page
                .items
                .into_iter()
                .map(with_cache_busted_cover)
                .collect(),
            ..page
        })
    }
}
