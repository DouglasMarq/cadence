use crate::NavidromeServer;
use crate::features::Features;

impl NavidromeServer {
    pub(crate) async fn features(&self) -> Features {
        match self.subsonic_server.server_info().await {
            Ok(info) => Features::from_server_info(info),
            Err(error) => {
                tracing::warn!(%error, "Navidrome feature detection failed, assuming no features");
                Features::default()
            }
        }
    }
}
