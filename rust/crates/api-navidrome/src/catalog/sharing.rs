use cadence_api::{ApiError, MusicServer, Result};
use cadence_core::model::Share;

use crate::NavidromeServer;

const SHARING_DISABLED: &str = "sharing is turned off on this Navidrome server (EnableSharing)";
const NOT_IMPLEMENTED_STATUS: i32 = 501;

fn sharing_error(error: ApiError) -> ApiError {
    match error {
        ApiError::Server {
            code: NOT_IMPLEMENTED_STATUS,
            ..
        } => ApiError::Unsupported(SHARING_DISABLED),
        other => other,
    }
}

impl NavidromeServer {
    pub(crate) async fn share_items(
        &self,
        item_ids: &[String],
        description: Option<&str>,
    ) -> Result<Share> {
        self.subsonic_server
            .create_share(item_ids, description)
            .await
            .map_err(sharing_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_share_endpoint_means_sharing_is_disabled() {
        let not_implemented = ApiError::Server {
            code: NOT_IMPLEMENTED_STATUS,
            message: "This endpoint is not implemented".into(),
        };
        assert!(matches!(
            sharing_error(not_implemented),
            ApiError::Unsupported(SHARING_DISABLED)
        ));
        assert!(matches!(
            sharing_error(ApiError::NotFound),
            ApiError::NotFound
        ));
        assert!(matches!(
            sharing_error(ApiError::Network("down".into())),
            ApiError::Network(_)
        ));
    }
}
