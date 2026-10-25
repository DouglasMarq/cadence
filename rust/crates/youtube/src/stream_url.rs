use crate::links::query_parameter;

const EXPIRY_PARAMETER: &str = "expire";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamUrl {
    pub url: String,
    pub expires_at_unix_seconds: Option<u64>,
}

impl StreamUrl {
    pub fn new(url: String) -> Self {
        let expires_at_unix_seconds = expiry_unix_seconds_of(&url);
        Self {
            url,
            expires_at_unix_seconds,
        }
    }
}

fn expiry_unix_seconds_of(url: &str) -> Option<u64> {
    let (_, query) = url.split_once('?')?;
    query_parameter(query, EXPIRY_PARAMETER)?.parse().ok()
}
