use cadence_api::{MusicServer, Result};
use cadence_core::model::{Page, Paging, Song, SongListQuery};

use crate::NavidromeServer;
use crate::catalog::song_years::spans_several_years;
use crate::features::Features;
use crate::native::{NativeSong, normalize_song};
use crate::query::{self, PageRange};

const ALBUM_IDS_BATCH_SIZE: usize = 500;
const COUNT_PAGING: Paging = Paging {
    start_index: 0,
    limit: 1,
};

impl NavidromeServer {
    pub(crate) fn songs_from_native(&self, songs: Vec<NativeSong>) -> Vec<Song> {
        songs
            .into_iter()
            .map(|song| normalize_song(song, self.server_id()))
            .collect()
    }

    pub(crate) async fn list_songs(
        &self,
        query: &SongListQuery,
        paging: Paging,
    ) -> Result<Page<Song>> {
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_song_page_in_years(query, paging, &features),
            self.subsonic_server.song_list(query, paging),
        )
        .await
    }

    pub(crate) async fn count_songs(&self, query: &SongListQuery) -> Result<usize> {
        let features = self.features().await;
        self.native_or_subsonic(
            self.native_song_count(query, &features),
            self.subsonic_server.song_count(query),
        )
        .await
    }

    async fn native_song_count(&self, query: &SongListQuery, features: &Features) -> Result<usize> {
        if spans_several_years(query) {
            return self.song_count_across_years(query, features).await;
        }
        let page = self.native_song_page(query, COUNT_PAGING, features).await?;
        Ok(page.total_count.unwrap_or(0))
    }

    async fn native_song_page_in_years(
        &self,
        query: &SongListQuery,
        paging: Paging,
        features: &Features,
    ) -> Result<Page<Song>> {
        match spans_several_years(query) {
            true => self.song_page_across_years(query, paging, features).await,
            false => self.native_song_page(query, paging, features).await,
        }
    }

    pub(crate) async fn fetch_song(&self, id: &str) -> Result<Song> {
        self.native_or_subsonic(self.native_song(id), self.subsonic_server.song(id))
            .await
    }

    pub(crate) async fn fetch_random_songs(
        &self,
        count: usize,
        genre_id: Option<&str>,
    ) -> Result<Vec<Song>> {
        let genre_name = match genre_id {
            Some(genre_id) => Some(self.genre_name(genre_id).await),
            None => None,
        };
        self.subsonic_server
            .random_songs(count, genre_name.as_deref())
            .await
    }

    async fn native_song(&self, id: &str) -> Result<Song> {
        let (song, _): (NativeSong, _) = self.native_get(&["song", id], &[]).await?;
        Ok(normalize_song(song, self.server_id()))
    }

    pub(crate) async fn native_song_page(
        &self,
        query: &SongListQuery,
        paging: Paging,
        features: &Features,
    ) -> Result<Page<Song>> {
        let range = PageRange::or_all(paging);
        let mut items = Vec::new();
        let mut total: Option<usize> = None;
        for batch in album_id_batches(&query.album_ids) {
            let batch_query = SongListQuery {
                album_ids: batch.to_vec(),
                ..query.clone()
            };
            let params = query::song_list(&batch_query, range, features);
            let (songs, count): (Vec<NativeSong>, _) = self.native_get(&["song"], &params).await?;
            items.extend(self.songs_from_native(songs));
            total = count.map(|count| total.unwrap_or(0) + count).or(total);
        }
        Ok(Page {
            items,
            start_index: paging.start_index,
            total_count: total,
        })
    }
}

fn album_id_batches(album_ids: &[String]) -> Vec<&[String]> {
    if album_ids.len() > ALBUM_IDS_BATCH_SIZE {
        return album_ids.chunks(ALBUM_IDS_BATCH_SIZE).collect();
    }
    vec![album_ids]
}
