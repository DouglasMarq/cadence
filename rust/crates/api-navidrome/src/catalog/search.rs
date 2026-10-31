use cadence_api::{MusicServer, Result};
use cadence_core::model::{AlbumListQuery, ArtistListQuery, Paging, SearchResults, SongListQuery};

use crate::NavidromeServer;

const SEARCH3_MINIMUM_BYTES: usize = 2;

impl NavidromeServer {
    pub(crate) async fn search_library(&self, term: &str, limit: usize) -> Result<SearchResults> {
        if term.len() >= SEARCH3_MINIMUM_BYTES {
            return self.subsonic_server.search(term, limit).await;
        }
        self.search_each_list(term, limit).await
    }

    async fn search_each_list(&self, term: &str, limit: usize) -> Result<SearchResults> {
        let paging = Paging {
            start_index: 0,
            limit,
        };
        let search = Some(term.to_owned());
        let song_query = SongListQuery {
            search: search.clone(),
            ..SongListQuery::default()
        };
        let album_query = AlbumListQuery {
            search: search.clone(),
            ..AlbumListQuery::default()
        };
        let artist_query = ArtistListQuery {
            search,
            ..ArtistListQuery::default()
        };
        Ok(SearchResults {
            songs: self.list_songs(&song_query, paging).await?.items,
            albums: self.list_albums(&album_query, paging).await?.items,
            artists: self.list_artists(&artist_query, paging).await?.items,
        })
    }
}
