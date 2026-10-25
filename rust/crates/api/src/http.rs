use std::time::Duration;

use reqwest::StatusCode;

use crate::error::{ApiError, Result};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ERROR_MESSAGE_CHARS: usize = 200;

pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(ApiError::from_network)
}

impl ApiError {
    pub fn from_network(error: reqwest::Error) -> Self {
        Self::Network(Box::new(error.without_url()))
    }

    pub fn from_http_status(status: StatusCode, body: &str) -> Self {
        match status {
            StatusCode::UNAUTHORIZED => Self::Auth(status_message(status, body)),
            StatusCode::FORBIDDEN => Self::NotAuthorized(status_message(status, body)),
            StatusCode::NOT_FOUND => Self::NotFound,
            _ => Self::Server {
                code: i32::from(status.as_u16()),
                message: status_message(status, body),
            },
        }
    }
}

fn status_message(status: StatusCode, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return status.to_string();
    }
    trimmed.chars().take(MAX_ERROR_MESSAGE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_the_default_settings_when_building_the_client_then_it_succeeds() {
        assert!(http_client().is_ok());
    }

    #[test]
    fn given_http_401_when_mapping_the_status_then_it_is_an_auth_failure_with_the_body() {
        let error = ApiError::from_http_status(StatusCode::UNAUTHORIZED, " bad token \n");
        assert!(matches!(error, ApiError::Auth(message) if message == "bad token"));
    }

    #[test]
    fn given_http_403_when_mapping_the_status_then_it_is_not_authorized() {
        let error = ApiError::from_http_status(StatusCode::FORBIDDEN, "admin only");
        assert!(matches!(error, ApiError::NotAuthorized(message) if message == "admin only"));
    }

    #[test]
    fn given_http_404_when_mapping_the_status_then_it_is_not_found() {
        let error = ApiError::from_http_status(StatusCode::NOT_FOUND, "missing");
        assert!(matches!(error, ApiError::NotFound));
    }

    #[test]
    fn given_an_empty_body_when_mapping_a_server_error_then_the_message_is_the_status() {
        let error = ApiError::from_http_status(StatusCode::BAD_GATEWAY, "  ");
        assert!(matches!(
            error,
            ApiError::Server { code: 502, message } if message == "502 Bad Gateway"
        ));
    }

    #[test]
    fn given_a_long_body_when_mapping_a_server_error_then_the_message_is_truncated() {
        let body = "x".repeat(MAX_ERROR_MESSAGE_CHARS * 2);
        let error = ApiError::from_http_status(StatusCode::INTERNAL_SERVER_ERROR, &body);
        assert!(matches!(
            error,
            ApiError::Server { code: 500, message } if message.len() == MAX_ERROR_MESSAGE_CHARS
        ));
    }
}
