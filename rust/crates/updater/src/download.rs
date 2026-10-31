use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use crate::checksum::Sha256Digest;
use crate::error::{Result, UpdateError, io_error};
use crate::feed::{ReleaseFeed, checked_send};
use crate::release::ReleaseAsset;

const MAX_PERCENT: u64 = 100;

pub struct Download<'a> {
    pub asset: &'a ReleaseAsset,
    pub expected: &'a Sha256Digest,
    pub destination: &'a Path,
}

struct PercentReporter<'a> {
    total: u64,
    received: u64,
    last_percent: Option<u8>,
    on_percent: &'a (dyn Fn(u8) + Sync),
}

impl PercentReporter<'_> {
    fn advance(&mut self, bytes: u64) {
        self.received = self.received.saturating_add(bytes);
        let percent = percent_of(self.received, self.total);
        if self.last_percent != Some(percent) {
            self.last_percent = Some(percent);
            (self.on_percent)(percent);
        }
    }
}

fn percent_of(received: u64, total: u64) -> u8 {
    let percent = received.saturating_mul(MAX_PERCENT) / total.max(1);
    u8::try_from(percent.min(MAX_PERCENT)).unwrap_or_default()
}

impl ReleaseFeed {
    pub(crate) async fn download(
        &self,
        download: &Download<'_>,
        on_percent: &(dyn Fn(u8) + Sync),
    ) -> Result<()> {
        let received = self.stream_to_file(download, on_percent).await?;
        if received == *download.expected {
            return Ok(());
        }
        Err(UpdateError::ChecksumMismatch(download.asset.name.clone()))
    }

    async fn stream_to_file(
        &self,
        download: &Download<'_>,
        on_percent: &(dyn Fn(u8) + Sync),
    ) -> Result<Sha256Digest> {
        let asset = download.asset;
        let mut response = checked_send(self.http.get(&asset.browser_download_url)).await?;
        let mut file = File::create(download.destination)
            .await
            .map_err(io_error("creating the download"))?;
        let mut hasher = Sha256::new();
        let mut reporter = PercentReporter {
            total: asset.size,
            received: 0,
            last_percent: None,
            on_percent,
        };
        while let Some(chunk) = response.chunk().await? {
            reporter.advance(chunk.len() as u64);
            if reporter.received > asset.size {
                return Err(UpdateError::TooLarge(asset.name.clone()));
            }
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(io_error("writing the download"))?;
        }
        file.sync_all()
            .await
            .map_err(io_error("writing the download"))?;
        Ok(Sha256Digest::finish(hasher))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn reported(total: u64, chunks: &[u64]) -> Vec<u8> {
        let percents = Mutex::new(Vec::new());
        let record = |percent| percents.lock().unwrap().push(percent);
        let mut reporter = PercentReporter {
            total,
            received: 0,
            last_percent: None,
            on_percent: &record,
        };
        for chunk in chunks {
            reporter.advance(*chunk);
        }
        percents.into_inner().unwrap()
    }

    #[test]
    fn given_chunks_when_downloading_then_each_new_percentage_is_reported_once() {
        assert_eq!(reported(200, &[100, 1, 1, 98]), [50, 51, 100]);
    }

    #[test]
    fn given_small_chunks_when_downloading_then_a_repeated_percentage_is_not_reported() {
        assert_eq!(reported(1000, &[1, 1, 8]), [0, 1]);
    }

    #[test]
    fn given_an_empty_asset_when_downloading_then_the_percentage_stays_in_range() {
        assert_eq!(percent_of(5, 0), 100);
    }

    #[test]
    fn given_more_bytes_than_declared_when_downloading_then_the_percentage_stays_in_range() {
        assert_eq!(percent_of(300, 100), 100);
    }
}
