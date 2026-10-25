use cadence_api::{ApiError, Result};
use cadence_core::model::ServerConfig;
use reqwest::{RequestBuilder, Response, Url};
use serde::de::DeserializeOwned;

pub fn base_url(config: &ServerConfig) -> Result<Url> {
    let url = Url::parse(&config.url).map_err(|error| ApiError::InvalidAddress(Box::new(error)))?;
    match url.cannot_be_a_base() {
        true => Err(ApiError::InvalidAddress(
            format!("{} cannot hold a path", url.scheme()).into(),
        )),
        false => Ok(url),
    }
}

pub fn endpoint(base_url: &Url, path: &[&str]) -> Url {
    let mut url = base_url.clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.pop_if_empty().extend(path);
    }
    url
}

pub async fn send(request: RequestBuilder) -> Result<Response> {
    let response = request.send().await.map_err(ApiError::from_network)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let message = response
        .text()
        .await
        .inspect_err(|error| tracing::debug!("reading the error body failed: {error}"))
        .unwrap_or_default();
    Err(ApiError::from_http_status(status, &message))
}

pub async fn decode<Body: DeserializeOwned>(response: Response) -> Result<Body> {
    let bytes = response.bytes().await.map_err(ApiError::from_network)?;
    serde_json::from_slice(&bytes).map_err(|error| ApiError::Decode(Box::new(error)))
}
