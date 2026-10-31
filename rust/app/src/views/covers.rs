use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use cadence_api::MusicServer;
use cadence_storage::cover_cache::DecodedImage;

use crate::covers::CoverProvider;
use crate::session::Session;

const VIEW_LEFT: &str = "the view that asked for this cover was left";

pub struct FocusedCovers {
    covers: Arc<dyn CoverProvider>,
    session: Session,
}

impl FocusedCovers {
    pub fn new(covers: Arc<dyn CoverProvider>, session: Session) -> Self {
        Self { covers, session }
    }
}

#[async_trait]
impl CoverProvider for FocusedCovers {
    async fn cover(
        &self,
        server: &dyn MusicServer,
        cover_art_id: &str,
        size: u32,
    ) -> Result<Arc<DecodedImage>> {
        self.session
            .view_work()
            .run(self.covers.cover(server, cover_art_id, size))
            .await
            .ok_or_else(|| anyhow!(VIEW_LEFT))?
    }

    async fn cover_from_url(
        &self,
        cache_key: &str,
        url: &str,
        size: u32,
    ) -> Result<Arc<DecodedImage>> {
        self.session
            .view_work()
            .run(self.covers.cover_from_url(cache_key, url, size))
            .await
            .ok_or_else(|| anyhow!(VIEW_LEFT))?
    }

    fn cover_file(
        &self,
        server: &dyn MusicServer,
        cover_art_id: &str,
        size: u32,
    ) -> Option<PathBuf> {
        self.covers.cover_file(server, cover_art_id, size)
    }
}
