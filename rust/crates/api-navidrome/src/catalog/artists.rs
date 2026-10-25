use cadence_api::{MusicServer, Result};
use cadence_core::model::{
    AlbumListQuery, AlbumSort, Artist, ArtistDetail, ArtistListQuery, Page, Paging, SortOrder,
};

use crate::NavidromeServer;
use crate::features::Features;
use crate::native::{NativeArtist, normalize_artist};
use crate::query::{self, PageRange, end_index_or_all};

impl NavidromeServer {
    pub(crate) async fn list_artists(
        &self,
        query: &ArtistListQuery,
        paging: Paging,
    ) -> Result<Page<Artist>> {
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_artist_page(query, paging, &features),
            self.subsonic_server.artist_list(query, paging),
        )
        .await
    }

    pub(crate) async fn fetch_artist_detail(&self, id: &str) -> Result<ArtistDetail> {
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_artist(id, &features),
            self.subsonic_server.artist(id),
        )
        .await
    }

    async fn native_artist_page(
        &self,
        query: &ArtistListQuery,
        paging: Paging,
        features: &Features,
    ) -> Result<Page<Artist>> {
        let params = query::artist_list(query, PageRange::bounded(paging), features);
        let (artists, total): (Vec<NativeArtist>, _) =
            self.native_get(&["artist"], &params).await?;
        Ok(Page {
            items: artists
                .into_iter()
                .map(|artist| normalize_artist(artist, self.server_id()))
                .collect(),
            start_index: paging.start_index,
            total_count: total,
        })
    }

    async fn native_artist(&self, id: &str, features: &Features) -> Result<ArtistDetail> {
        let (artist, _): (NativeArtist, _) = self.native_get(&["artist", id], &[]).await?;
        let albums = self
            .native_album_page(
                &artist_albums_query(id),
                PageRange {
                    start: 0,
                    end: end_index_or_all(0, 0),
                },
                features,
            )
            .await?
            .items;
        let artist = normalize_artist(artist, self.server_id());
        let top_songs = self.subsonic_server.top_songs(id, &artist.name).await;
        Ok(ArtistDetail {
            artist,
            albums,
            top_songs,
        })
    }
}

fn artist_albums_query(artist_id: &str) -> AlbumListQuery {
    AlbumListQuery {
        sort: AlbumSort::Year,
        order: SortOrder::Desc,
        artist_ids: vec![artist_id.to_owned()],
        ..AlbumListQuery::default()
    }
}
