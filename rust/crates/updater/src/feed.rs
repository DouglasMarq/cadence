use std::time::Duration;

use cadence_core::capped_read::{CappedReadError, read_capped};
use reqwest::header::ACCEPT;
use reqwest::{RequestBuilder, Response, StatusCode};

use crate::checksum::{Sha256Digest, listed_checksum};
use crate::error::{Result, UpdateError};
use crate::release::{Offer, Release};

pub const USER_AGENT: &str = concat!(
    "Cadence/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/DouglasMarq/cadence)"
);

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/DouglasMarq/cadence/releases/latest";
const GITHUB_JSON: &str = "application/vnd.github+json";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_METADATA_BYTES: usize = 1024 * 1024;

pub struct ReleaseFeed {
    pub(crate) http: reqwest::Client,
    latest_url: String,
}

pub fn https_only_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()?)
}

impl ReleaseFeed {
    pub fn new(http: reqwest::Client, latest_url: impl Into<String>) -> Self {
        Self {
            http,
            latest_url: latest_url.into(),
        }
    }

    pub fn github() -> Result<Self> {
        Ok(Self::new(https_only_client()?, LATEST_RELEASE_URL))
    }

    pub async fn latest(&self) -> Result<Option<Release>> {
        let request = self.http.get(&self.latest_url).header(ACCEPT, GITHUB_JSON);
        let response = request.send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            tracing::debug!("no release has been published yet");
            return Ok(None);
        }
        let body = capped_body(successful(response)?, &self.latest_url).await?;
        Ok(Some(serde_json::from_slice(&body)?))
    }

    pub(crate) async fn checksum_of(&self, offer: &Offer) -> Result<Sha256Digest> {
        let url = &offer.checksums.browser_download_url;
        let body = capped_body(checked_send(self.http.get(url)).await?, url).await?;
        listed_checksum(&String::from_utf8_lossy(&body), &offer.asset.name)
            .ok_or_else(|| UpdateError::MissingChecksum(offer.asset.name.clone()))
    }
}

pub(crate) async fn checked_send(request: RequestBuilder) -> Result<Response> {
    successful(request.send().await?)
}

fn successful(response: Response) -> Result<Response> {
    match response.status() {
        status if status.is_success() => Ok(response),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Err(UpdateError::RateLimited),
        status => Err(UpdateError::Status(status)),
    }
}

async fn capped_body(mut response: Response, url: &str) -> Result<Vec<u8>> {
    read_capped(
        &mut response,
        |response| Box::pin(response.chunk()),
        MAX_METADATA_BYTES,
    )
    .await
    .map_err(|error| match error {
        CappedReadError::Read(error) => UpdateError::Network(error),
        CappedReadError::TooLarge => UpdateError::TooLarge(url.to_owned()),
    })
}
