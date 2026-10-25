use std::time::Duration;

use cadence_core::model::{Page, Song};

use super::delivery::Cancelled;
use super::pages::PageNumber;
use super::runner::StreamRunner;
use crate::backoff::exponential_delay;

const MAX_RETRY_MULTIPLIER: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub first_delay: Duration,
}

impl RetryPolicy {
    pub const PATIENT: Self = Self {
        attempts: 3,
        first_delay: Duration::from_millis(400),
    };

    fn delay_after(self, failed_attempts: u32) -> Duration {
        let longest_delay = self.first_delay.saturating_mul(MAX_RETRY_MULTIPLIER);
        exponential_delay(self.first_delay, failed_attempts, longest_delay)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStop {
    Cancelled,
    Failed(String),
}

impl From<Cancelled> for StreamStop {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

impl StreamRunner {
    pub(super) async fn fetch(&self, page: PageNumber) -> Result<Page<Song>, StreamStop> {
        let mut failed_attempts = 0;
        loop {
            let error = match self.fetcher.fetch(page.paging()).await {
                Ok(fetched) => return Ok(fetched),
                Err(error) => error,
            };
            failed_attempts += 1;
            tracing::warn!(
                "loading rows from {} failed (attempt {failed_attempts}): {error}",
                page.first_row().0
            );
            if failed_attempts >= self.retry.attempts {
                return Err(StreamStop::Failed(error.to_string()));
            }
            tokio::time::sleep(self.retry.delay_after(failed_attempts)).await;
        }
    }
}
