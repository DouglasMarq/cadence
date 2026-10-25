use cadence_api::{MusicServer, Result};
use cadence_core::model::{ServerPlayQueue, Song};
use reqwest::Method;

use crate::NavidromeServer;
use crate::features::Features;
use crate::native::{NativeQueue, SaveQueueRequest};

fn prefers_native_save(features: &Features) -> bool {
    features.supports_native_play_queue() && !features.has_index_based_queue()
}

impl NavidromeServer {
    pub(crate) async fn store_play_queue(
        &self,
        song_ids: &[String],
        current_song_id: Option<&str>,
        position_ms: u64,
    ) -> Result<()> {
        let features = self.features().await;
        let subsonic_save =
            self.subsonic_server
                .save_play_queue(song_ids, current_song_id, position_ms);
        if !prefers_native_save(&features) {
            return subsonic_save.await;
        }
        let request = SaveQueueRequest {
            ids: song_ids,
            position: position_ms,
            current: current_song_id
                .and_then(|current| song_ids.iter().position(|id| id == current)),
        };
        self.native_or_subsonic(self.native_save_queue(&request), subsonic_save)
            .await
    }

    pub(crate) async fn fetch_play_queue(&self) -> Result<Option<ServerPlayQueue>> {
        let features = self.features().await;
        if !features.supports_native_play_queue() {
            return self.subsonic_server.play_queue().await;
        }
        self.native_or_subsonic(self.native_play_queue(), self.subsonic_server.play_queue())
            .await
    }

    async fn native_save_queue(&self, request: &SaveQueueRequest<'_>) -> Result<()> {
        self.native_with_body(Method::POST, &["queue"], request)
            .await
            .map(|_| ())
    }

    async fn native_play_queue(&self) -> Result<Option<ServerPlayQueue>> {
        let (queue, _): (NativeQueue, _) = self.native_get(&["queue"], &[]).await?;
        let songs = self.songs_from_native(queue.items.unwrap_or_default());
        if songs.is_empty() {
            return Ok(None);
        }
        Ok(Some(ServerPlayQueue {
            current_song_id: song_id_at(&songs, queue.current.unwrap_or(0)),
            songs,
            position_ms: queue.position.unwrap_or(0),
        }))
    }
}

fn song_id_at(songs: &[Song], index: u32) -> Option<String> {
    let index = usize::try_from(index).ok()?;
    songs.get(index).map(|song| song.id.clone())
}
