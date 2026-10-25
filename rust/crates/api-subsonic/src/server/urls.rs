use cadence_api::StreamOptions;

use super::SubsonicServer;
use crate::client::{ClientIdentity, encode_uri_component};

impl SubsonicServer {
    fn item_url(&self, endpoint: &str, id: &str) -> String {
        format!(
            "{}/rest/{endpoint}?id={}",
            self.client.base(),
            encode_uri_component(id)
        )
    }

    pub(super) fn build_stream_url(&self, song_id: &str, options: &StreamOptions) -> String {
        let format = options
            .format
            .as_ref()
            .map(|format| format!("&format={}", encode_uri_component(format.as_str())));
        let bitrate = options
            .max_bitrate_kbps
            .map(|bitrate| format!("&maxBitRate={bitrate}"));
        let ClientIdentity {
            api_version,
            client_name,
        } = self.client.identity();
        format!(
            "{}&v={api_version}&c={client_name}&{}{}{}",
            self.item_url("stream.view", song_id),
            self.client.auth(),
            format.unwrap_or_default(),
            bitrate.unwrap_or_default()
        )
    }

    pub(super) fn build_cover_art_url(&self, cover_art_id: &str, size: Option<u32>) -> String {
        let size = size
            .filter(|edge| *edge > 0)
            .map(|size| format!("&size={size}"));
        let ClientIdentity {
            api_version,
            client_name,
        } = self.client.identity();
        format!(
            "{}&{}&v={api_version}&c={client_name}{}",
            self.item_url("getCoverArt.view", cover_art_id),
            self.client.auth(),
            size.unwrap_or_default()
        )
    }
}
