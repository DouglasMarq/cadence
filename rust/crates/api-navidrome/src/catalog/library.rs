use cadence_api::{MusicServer, Result};
use cadence_core::model::Genre;

use crate::NavidromeServer;
use crate::native::{NativeGenre, NativeTag, genre_with_counts, normalize_genre};
use crate::query::QueryParams;

impl NavidromeServer {
    pub(crate) async fn list_genres(&self) -> Result<Vec<Genre>> {
        let features = self.features().await;
        if !features.has_big_feature_release() {
            return self
                .native_or_subsonic(
                    self.legacy_native_genres(),
                    self.subsonic_server.genre_list(),
                )
                .await;
        }
        self.native_or_subsonic(self.native_genres(), self.subsonic_server.genre_list())
            .await
    }

    pub(crate) async fn genre_name(&self, genre_id: &str) -> String {
        match self.list_genres().await {
            Ok(genres) => genres
                .into_iter()
                .find(|genre| genre.id == genre_id)
                .map_or_else(|| genre_id.to_owned(), |genre| genre.name),
            Err(error) => {
                tracing::debug!(%error, genre_id, "genre lookup failed, sending the id as the name");
                genre_id.to_owned()
            }
        }
    }

    async fn legacy_native_genres(&self) -> Result<Vec<Genre>> {
        let params: QueryParams = vec![("_sort", "name".into()), ("_order", "ASC".into())];
        let (genres, _): (Vec<NativeGenre>, _) = self.native_get(&["genre"], &params).await?;
        let counted = self
            .subsonic_server
            .genre_list()
            .await
            .unwrap_or_else(|error| {
                tracing::debug!(%error, "genre counts unavailable");
                Vec::new()
            });
        Ok(genres
            .into_iter()
            .map(|genre| genre_with_counts(genre, &counted))
            .collect())
    }

    async fn native_genres(&self) -> Result<Vec<Genre>> {
        let params: QueryParams = vec![
            ("_sort", "tagValue".into()),
            ("_order", "ASC".into()),
            ("tag_name", "genre".into()),
        ];
        let (tags, _): (Vec<NativeTag>, _) = self.native_get(&["tag"], &params).await?;
        Ok(tags.into_iter().map(normalize_genre).collect())
    }
}
