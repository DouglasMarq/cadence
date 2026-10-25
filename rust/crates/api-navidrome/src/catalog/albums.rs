use cadence_api::{MusicServer, Result};
use cadence_core::model::{Album, AlbumDetail, AlbumListQuery, Page, Paging};

use crate::NavidromeServer;
use crate::catalog::album_years::YearRange;
use crate::features::Features;
use crate::native::{NativeAlbum, NativeSong, normalize_album};
use crate::query::{self, PageRange};

impl NavidromeServer {
    pub(crate) async fn list_albums(
        &self,
        query: &AlbumListQuery,
        paging: Paging,
    ) -> Result<Page<Album>> {
        if let Some(range) = YearRange::of(query) {
            return self.album_page_in_year_range(query, range, paging).await;
        }
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_album_page(query, PageRange::bounded(paging), &features),
            self.subsonic_server.album_list(query, paging),
        )
        .await
    }

    pub(crate) async fn count_albums(&self, query: &AlbumListQuery) -> Result<usize> {
        if let Some(range) = YearRange::of(query) {
            return self.count_albums_in_year_range(query, range).await;
        }
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_album_count(query, &features),
            self.subsonic_server.album_count(query),
        )
        .await
    }

    async fn native_album_count(
        &self,
        query: &AlbumListQuery,
        features: &Features,
    ) -> Result<usize> {
        let page = self
            .native_album_page(query, PageRange { start: 0, end: 1 }, features)
            .await?;
        Ok(page.total_count.unwrap_or(0))
    }

    pub(crate) async fn fetch_album_detail(&self, id: &str) -> Result<AlbumDetail> {
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_album(id, &features),
            self.subsonic_server.album(id),
        )
        .await
    }

    pub(crate) async fn native_album_page(
        &self,
        query: &AlbumListQuery,
        range: PageRange,
        features: &Features,
    ) -> Result<Page<Album>> {
        let params = query::album_list(query, range, features);
        let (albums, total): (Vec<NativeAlbum>, _) = self.native_get(&["album"], &params).await?;
        Ok(Page {
            items: albums
                .into_iter()
                .map(|album| normalize_album(album, self.server_id()))
                .collect(),
            start_index: range.start,
            total_count: total,
        })
    }

    async fn native_album(&self, id: &str, features: &Features) -> Result<AlbumDetail> {
        let (album, _): (NativeAlbum, _) = self.native_get(&["album", id], &[]).await?;
        let (songs, _): (Vec<NativeSong>, _) = self
            .native_get(&["song"], &query::album_songs(id, features))
            .await?;
        Ok(AlbumDetail {
            album: normalize_album(album, self.server_id()),
            songs: self.songs_from_native(songs),
        })
    }
}
