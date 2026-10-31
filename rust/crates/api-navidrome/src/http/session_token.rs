use std::sync::PoisonError;

use cadence_api::ApiError;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;

use crate::NavidromeServer;

pub(crate) const SESSION_TOKEN_HEADER: &str = "x-nd-authorization";

impl NavidromeServer {
    pub(crate) fn current_session_token(&self) -> Option<String> {
        self.session_token
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn saved_password(&self) -> Option<String> {
        self.password
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn forget_password_if_rejected(&self, error: &ApiError) {
        if !matches!(error, ApiError::Auth(_)) {
            return;
        }
        *self
            .password
            .write()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub(crate) fn can_use_native_api(&self) -> bool {
        self.current_session_token().is_some() || self.saved_password().is_some()
    }

    pub(crate) fn replace_session_token(&self, token: Option<String>) {
        *self
            .session_token
            .write()
            .unwrap_or_else(PoisonError::into_inner) = token;
    }

    pub(crate) fn forget_rejected_session_token(&self, status: StatusCode, rejected_token: &str) {
        if status != StatusCode::UNAUTHORIZED {
            return;
        }
        let mut session_token = self
            .session_token
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if session_token.as_deref() == Some(rejected_token) {
            *session_token = None;
        }
    }

    pub(crate) fn capture_refreshed_token(&self, headers: &HeaderMap) {
        let Some(token) = refreshed_token(headers) else {
            return;
        };
        self.replace_session_token(Some(token.to_owned()));
    }
}

fn refreshed_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(SESSION_TOKEN_HEADER)
        .and_then(|header| header.to_str().ok())
        .map(str::trim)
        .map(|header| header.strip_prefix("Bearer ").unwrap_or(header).trim())
        .filter(|token| !token.is_empty())
}
