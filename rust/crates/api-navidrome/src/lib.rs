mod catalog;
mod connection;
mod conversions;
mod features;
mod http;
mod lenient;
mod music_server;
mod native;
mod query;

use std::sync::RwLock;

use cadence_api_subsonic::SubsonicServer;
use cadence_core::model::ServerConfig;
use tokio::sync::Mutex;

pub struct NavidromeServer {
    config: ServerConfig,
    base_url: String,
    http_client: reqwest::Client,
    subsonic_auth: String,
    session_token: RwLock<Option<String>>,
    password: RwLock<Option<String>>,
    session_renewal: Mutex<()>,
    subsonic_server: SubsonicServer,
}
