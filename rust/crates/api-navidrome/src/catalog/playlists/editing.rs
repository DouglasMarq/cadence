use std::future::Future;

use cadence_api::{ApiError, MusicServer, Result};
use cadence_core::model::Playlist;
use reqwest::Method;

use super::covers::with_cache_busted_cover;
use crate::NavidromeServer;
use crate::native::{CreatePlaylistRequest, NativeCreatedId, TrackIdsRequest};
use crate::query::QueryParams;

impl NavidromeServer {
    pub(crate) async fn create_playlist_with_songs(
        &self,
        name: &str,
        song_ids: &[String],
    ) -> Result<Playlist> {
        self.changing_playlists(self.native_or_subsonic(
            self.native_create_playlist(name, song_ids),
            self.subsonic_create_playlist(name, song_ids),
        ))
        .await
    }

    pub(crate) async fn add_songs_to_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[String],
    ) -> Result<()> {
        if song_ids.is_empty() {
            return Ok(());
        }
        self.changing_playlists(self.native_or_subsonic(
            self.native_add_tracks(playlist_id, song_ids),
            self.subsonic_server.add_to_playlist(playlist_id, song_ids),
        ))
        .await
    }

    pub(crate) async fn remove_playlist_entries(
        &self,
        playlist_id: &str,
        indexes: &[usize],
    ) -> Result<()> {
        if indexes.is_empty() {
            return Ok(());
        }
        self.changing_playlists(
            self.native_or_subsonic(
                self.native_remove_tracks(playlist_id, indexes),
                self.subsonic_server
                    .remove_from_playlist(playlist_id, indexes),
            ),
        )
        .await
    }

    pub(crate) async fn remove_playlist(&self, id: &str) -> Result<()> {
        let native = async {
            self.native(Method::DELETE, &["playlist", id], &[])
                .await
                .map(|_| ())
        };
        self.changing_playlists(
            self.native_or_subsonic(native, self.subsonic_server.delete_playlist(id)),
        )
        .await
    }

    async fn native_create_playlist(&self, name: &str, song_ids: &[String]) -> Result<Playlist> {
        let request = CreatePlaylistRequest { name, rules: None };
        let created: NativeCreatedId = self
            .native_with_body(Method::POST, &["playlist"], &request)
            .await?
            .json()?;
        if !song_ids.is_empty() {
            self.native_add_tracks(&created.id, song_ids).await?;
        }
        self.native_playlist(&created.id).await
    }

    async fn subsonic_create_playlist(&self, name: &str, song_ids: &[String]) -> Result<Playlist> {
        let playlist = self.subsonic_server.create_playlist(name, song_ids).await?;
        Ok(with_cache_busted_cover(playlist))
    }

    async fn native_add_tracks(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        self.native_with_body(
            Method::POST,
            &["playlist", playlist_id, "tracks"],
            &TrackIdsRequest { ids: song_ids },
        )
        .await
        .map(|_| ())
    }

    async fn native_remove_tracks(&self, playlist_id: &str, indexes: &[usize]) -> Result<()> {
        let features = self.features().await;
        let tracks = self.native_playlist_tracks(playlist_id, &features).await?;
        let params = indexes
            .iter()
            .map(|index| {
                tracks
                    .get(*index)
                    .map(|track| ("id", track.id.clone()))
                    .ok_or(ApiError::NotFound)
            })
            .collect::<Result<QueryParams>>()?;
        self.native(
            Method::DELETE,
            &["playlist", playlist_id, "tracks"],
            &params,
        )
        .await
        .map(|_| ())
    }

    async fn changing_playlists<Output>(
        &self,
        change: impl Future<Output = Result<Output>>,
    ) -> Result<Output> {
        let outcome = change.await;
        self.subsonic_server.forget_cached_playlists();
        outcome
    }
}
