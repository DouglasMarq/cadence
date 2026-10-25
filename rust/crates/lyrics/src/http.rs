use std::time::Duration;

use cadence_core::capped_read::{CappedReadError, read_capped};
use serde::de::DeserializeOwned;

use crate::{LyricsError, Result};

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

async fn send(request: reqwest::RequestBuilder) -> Result<Option<reqwest::Response>> {
    let response = request.send().await.map_err(network_error)?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(LyricsError::Network(format!("HTTP {status}").into()));
    }
    Ok(Some(response))
}

pub(crate) async fn send_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
) -> Result<Option<T>> {
    let Some(response) = send(request).await? else {
        return Ok(None);
    };
    let body = capped_body(response).await?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| LyricsError::Decode(Box::new(error)))
}

pub(crate) async fn send_text(request: reqwest::RequestBuilder) -> Result<Option<String>> {
    let Some(response) = send(request).await? else {
        return Ok(None);
    };
    let body = capped_body(response).await?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

async fn capped_body(mut response: reqwest::Response) -> Result<Vec<u8>> {
    read_capped(
        &mut response,
        |response| Box::pin(response.chunk()),
        MAX_RESPONSE_BYTES,
    )
    .await
    .map_err(|error| match error {
        CappedReadError::Read(error) => network_error(error),
        CappedReadError::TooLarge => LyricsError::Decode(
            format!("the response is larger than {MAX_RESPONSE_BYTES} bytes").into(),
        ),
    })
}

fn network_error(error: reqwest::Error) -> LyricsError {
    LyricsError::Network(Box::new(error))
}
