use async_trait::async_trait;
use cadence_storage::settings::YouTubeSettings;
use cadence_youtube::music::{CollectionPage, MusicError, MusicSong};

use super::feed::{DiscoverFeed, FeedRequest};

#[async_trait]
pub trait DiscoverSource: Send + Sync {
    async fn feed(
        &self,
        request: FeedRequest,
        settings: YouTubeSettings,
    ) -> Result<DiscoverFeed, MusicError>;

    async fn collection(
        &self,
        browse_id: String,
        settings: YouTubeSettings,
    ) -> Result<CollectionPage, MusicError>;

    async fn search(
        &self,
        query: String,
        settings: YouTubeSettings,
    ) -> Result<Vec<MusicSong>, MusicError>;

    async fn radio(
        &self,
        video_id: String,
        settings: YouTubeSettings,
    ) -> Result<Vec<MusicSong>, MusicError>;
}
