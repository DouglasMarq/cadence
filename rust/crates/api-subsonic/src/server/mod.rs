mod albums;
mod annotations;
mod artists;
mod auth;
mod cached_lists;
mod counting;
mod folders;
mod library;
mod list_caches;
mod music_server;
mod pages;
mod play_queue;
mod playlist_cache;
mod playlists;
mod radio;
mod requests;
mod scanning;
mod sharing;
mod songs;
mod transport;
mod urls;

use std::time::Duration;

use cadence_api::{LatestCache, Result};
use cadence_core::model::{FolderId, ServerConfig};
use tokio::sync::OnceCell;

use crate::client::{Client, ClientIdentity, Params, RequestEncoding, SUBSONIC_IDENTITY};
use crate::extensions::Extensions;
use crate::normalize::FolderListing;
use crate::server_info::ServerInfo;
use crate::types::{OpenSubsonicExtensions, Ping};
use list_caches::ListCaches;
use playlist_cache::{PlaylistCache, playlist_cache};

const FOLDER_TIME_TO_LIVE: Duration = Duration::from_secs(30);

pub struct SubsonicServer {
    config: ServerConfig,
    client: Client,
    server_info: OnceCell<ServerInfo>,
    list_caches: ListCaches,
    playlist_cache: PlaylistCache,
    latest_folder: LatestCache<FolderId, FolderListing>,
}

fn is_open_subsonic(ping: &Ping) -> bool {
    ping.is_open_subsonic.unwrap_or(false) && ping.server_version.is_some()
}

impl SubsonicServer {
    fn new(config: ServerConfig, auth: String) -> Result<Self> {
        Self::with_identity(config, auth, SUBSONIC_IDENTITY)
    }

    pub fn with_identity(
        config: ServerConfig,
        auth: String,
        identity: ClientIdentity,
    ) -> Result<Self> {
        let client = Client::new(&config.url, auth, identity)?;
        Ok(Self {
            config,
            client,
            server_info: OnceCell::new(),
            list_caches: ListCaches::new(),
            playlist_cache: playlist_cache(),
            latest_folder: LatestCache::new(FOLDER_TIME_TO_LIVE),
        })
    }

    pub async fn extensions(&self) -> Result<&Extensions> {
        Ok(&self.server_info().await?.extensions)
    }

    pub async fn server_info(&self) -> Result<&ServerInfo> {
        self.server_info
            .get_or_try_init(|| self.detect_server_info())
            .await
    }

    async fn detect_server_info(&self) -> Result<ServerInfo> {
        let ping: Ping = self
            .client
            .call("ping.view", &Params::new(), RequestEncoding::QueryString)
            .await?
            .decode()?;
        let extensions = match is_open_subsonic(&ping) {
            true => self.detect_extensions().await?,
            false => Extensions::default(),
        };
        Ok(ServerInfo {
            version: ping.server_version,
            extensions,
        })
    }

    async fn detect_extensions(&self) -> Result<Extensions> {
        let response: OpenSubsonicExtensions = self
            .client
            .call(
                "getOpenSubsonicExtensions.view",
                &Params::new(),
                RequestEncoding::QueryString,
            )
            .await?
            .decode()?;
        Ok(Extensions::from_response(response))
    }
}
