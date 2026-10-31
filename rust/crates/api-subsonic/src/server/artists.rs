use cadence_api::{ArtistPartSink, Result};
use cadence_core::model::{
    Album, Artist, ArtistDetail, ArtistListQuery, ArtistPart, Page, Paging, Song,
};

use super::SubsonicServer;
use super::pages::{cloned_page, non_empty_search, wants_favorites_only};
use crate::client::Params;
use crate::normalize;
use crate::sort::sort_artists;
use crate::types::{ArtistInfo2, GetArtist, SubsonicArtist, TopSongs};

fn matching_artists<'index>(
    index: &'index [SubsonicArtist],
    query: &ArtistListQuery,
) -> Vec<&'index SubsonicArtist> {
    let lowercase_term = non_empty_search(query.search.as_ref()).map(str::to_lowercase);
    let is_favorites_only = wants_favorites_only(query.is_favorite);
    index
        .iter()
        .filter(|artist| {
            lowercase_term
                .as_ref()
                .is_none_or(|term| artist.name.to_lowercase().contains(term))
        })
        .filter(|artist| !is_favorites_only || artist.starred.is_starred())
        .collect()
}

fn biography_or_none(artist_info: Result<ArtistInfo2>) -> Option<String> {
    artist_info
        .inspect_err(|error| tracing::warn!(%error, "artist biography unavailable"))
        .ok()
        .and_then(|response| response.artist_info2)
        .and_then(|info| info.biography)
        .filter(|biography| !biography.is_empty())
}

impl SubsonicServer {
    pub(super) async fn fetch_artist_page(
        &self,
        query: &ArtistListQuery,
        paging: Paging,
    ) -> Result<Page<Artist>> {
        let index = self.artist_index(query.music_folder_id.as_deref()).await?;
        let mut artists = matching_artists(&index, query);
        sort_artists(&mut artists, query.sort, query.order);
        let page = cloned_page(Page::from_all(artists, paging));
        Ok(Page {
            items: self.normalize_artists(page.items),
            start_index: page.start_index,
            total_count: page.total_count,
        })
    }

    async fn fetch_artist_profile(&self, id: &str) -> Result<(Artist, Vec<Album>)> {
        let response: GetArtist = self
            .fetch("getArtist.view", Params::new().add("id", id))
            .await?;
        let mut raw_artist = response.artist;
        let albums = self.normalize_albums(raw_artist.album.take().unwrap_or_default());
        Ok((normalize::artist(raw_artist, self.server_id()), albums))
    }

    async fn fetch_biography(&self, id: &str) -> Option<String> {
        let artist_info = self
            .fetch::<ArtistInfo2>("getArtistInfo2.view", Params::new().add("id", id))
            .await;
        biography_or_none(artist_info)
    }

    pub(super) async fn fetch_artist_detail(&self, id: &str) -> Result<ArtistDetail> {
        let (mut artist, albums) = self.fetch_artist_profile(id).await?;
        let (top_songs, biography) =
            futures_util::future::join(self.top_songs(id, &artist.name), self.fetch_biography(id))
                .await;
        artist.biography = biography;
        Ok(ArtistDetail {
            artist,
            albums,
            top_songs,
        })
    }

    pub(super) async fn deliver_artist_parts(
        &self,
        id: &str,
        deliver: ArtistPartSink<'_>,
    ) -> Result<()> {
        let (artist, albums) = self.fetch_artist_profile(id).await?;
        let artist_name = artist.name.clone();
        deliver(ArtistPart::Profile(artist));
        deliver(ArtistPart::Albums(albums));
        futures_util::future::join(
            async { deliver(ArtistPart::TopSongs(self.top_songs(id, &artist_name).await)) },
            async { deliver(ArtistPart::Biography(self.fetch_biography(id).await)) },
        )
        .await;
        Ok(())
    }

    pub async fn top_songs(&self, artist_id: &str, artist_name: &str) -> Vec<Song> {
        let params = self.top_songs_params(artist_id, artist_name).await;
        self.top_songs_or_empty(self.fetch("getTopSongs.view", params).await)
    }

    async fn top_songs_params(&self, id: &str, name: &str) -> Params {
        let lookup_by_id = self
            .extensions()
            .await
            .inspect_err(|error| tracing::warn!(%error, "server extensions unavailable"))
            .is_ok_and(|extensions| extensions.has_top_songs_by_artist_id);
        match lookup_by_id {
            true => Params::new().add("id", id),
            false => Params::new().add("artist", name),
        }
    }

    fn top_songs_or_empty(&self, top_songs: Result<TopSongs>) -> Vec<Song> {
        top_songs
            .inspect_err(|error| tracing::warn!(%error, "artist top songs unavailable"))
            .map(|response| {
                self.normalize_songs(
                    response
                        .top_songs
                        .and_then(|container| container.song)
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_default()
    }
}
