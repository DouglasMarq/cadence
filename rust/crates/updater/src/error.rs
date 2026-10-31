use std::io;
use std::path::PathBuf;

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("GitHub refused the request because of its rate limit")]
    RateLimited,
    #[error("the server answered {0}")]
    Status(StatusCode),
    #[error("the release description could not be read: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("{0} is larger than expected")]
    TooLarge(String),
    #[error("\"{tag}\" is not a version: {source}")]
    InvalidVersion { tag: String, source: semver::Error },
    #[error("{0} is not listed in the release checksums")]
    MissingChecksum(String),
    #[error("the checksum of {0} does not match")]
    ChecksumMismatch(String),
    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        source: io::Error,
    },
    #[error("extracting the update failed: {0}")]
    Extract(String),
    #[error("the downloaded app is not valid: {0}")]
    InvalidBundle(String),
    #[error("this installation cannot be updated in place")]
    NotInstallable,
    #[error(
        "installing failed and the previous version could not be put back, it is at {}: {source}",
        backup.display()
    )]
    RollbackFailed { backup: PathBuf, source: io::Error },
    #[error("the update stopped unexpectedly: {0}")]
    Interrupted(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, UpdateError>;

pub(crate) fn io_error(context: &'static str) -> impl Fn(io::Error) -> UpdateError {
    move |source| UpdateError::Io { context, source }
}
