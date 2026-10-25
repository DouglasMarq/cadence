use cadence_core::queue::QueueTrack;
use cadence_core::unix_time::unix_now_ms;

use super::Controller;

impl Controller {
    pub(super) fn start_scrobbling(&mut self, track: &QueueTrack) {
        let Some(song_id) = track.server_song_id() else {
            self.scrobble_tracker.stop();
            return;
        };
        self.scrobble_tracker
            .start(song_id, track.duration_ms, unix_now_ms());
        self.send_scrobble(song_id.to_owned(), false, None);
    }

    pub(super) fn send_scrobble(
        &self,
        song_id: String,
        is_submission: bool,
        timestamp_ms: Option<i64>,
    ) {
        if !self.settings.playback.is_scrobbling_enabled {
            return;
        }
        let Some(server) = self.deps.session.get() else {
            return;
        };
        tokio::spawn(async move {
            if let Err(error) = server.scrobble(&song_id, is_submission, timestamp_ms).await {
                tracing::warn!("scrobble failed: {error}");
            }
        });
    }
}
