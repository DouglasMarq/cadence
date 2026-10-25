use cadence_api::{ApiError, ErrorCause};
use reqwest::StatusCode;
use serde::Deserialize;

const MISSING_DATA_MESSAGE: &str = "data not found";

#[derive(Deserialize)]
struct NativeErrorBody {
    error: String,
}

fn reports_missing_data(body: &[u8]) -> bool {
    serde_json::from_slice::<NativeErrorBody>(body)
        .is_ok_and(|parsed| parsed.error == MISSING_DATA_MESSAGE)
}

pub(crate) fn invalid_address_error(reason: impl Into<ErrorCause>) -> ApiError {
    ApiError::InvalidAddress(reason.into())
}

pub(crate) fn decode_error(error: serde_json::Error) -> ApiError {
    ApiError::Decode(Box::new(error))
}

pub(crate) fn missing_session_error() -> ApiError {
    ApiError::Auth("Navidrome session token is missing or expired".into())
}

pub(crate) fn http_status_error(status: StatusCode, body: &[u8]) -> ApiError {
    let is_auth_failure = matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN);
    if !is_auth_failure && reports_missing_data(body) {
        return ApiError::NotFound;
    }
    ApiError::from_http_status(status, &String::from_utf8_lossy(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_servers_report_missing_items_as_server_errors_with_a_body() {
        let error = http_status_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            br#"{"error":"data not found"}"#,
        );
        assert!(matches!(error, ApiError::NotFound));
    }

    #[test]
    fn given_http_403_returns_not_authorized() {
        let error = http_status_error(StatusCode::FORBIDDEN, b"admin only");
        assert!(matches!(error, ApiError::NotAuthorized(message) if message == "admin only"));
    }

    #[test]
    fn other_server_errors_keep_their_message() {
        let error = http_status_error(StatusCode::INTERNAL_SERVER_ERROR, b"boom");
        assert!(matches!(error, ApiError::Server { code: 500, .. }));
    }
}
