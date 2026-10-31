use std::sync::RwLock;

use cadence_api::{ApiError, Credential, Result, http_client};
use cadence_api_subsonic::SubsonicServer;
use cadence_core::model::ServerConfig;
use cadence_core::web_url::trimmed_base_url;
use reqwest::StatusCode;
use tokio::sync::{Mutex, watch};

use crate::NavidromeServer;
use crate::http::{NAVIDROME_IDENTITY, decode_error, query_string};
use crate::native::{LoginRequest, NativeAuth};

impl NavidromeServer {
    pub async fn login(config: ServerConfig, password: &str) -> Result<(Self, Credential)> {
        let http_client = http_client()?;
        let auth = request_session(&http_client, &config, password).await?;
        let credential = Credential::Navidrome {
            subsonic: subsonic_auth_query(
                &config.username,
                auth.subsonic_salt,
                auth.subsonic_token,
            ),
            nd_token: Some(auth.token),
            password: Some(password.to_owned()),
        };
        let server = Self::from_parts(config, http_client, &credential)?;
        Ok((server, credential))
    }

    pub fn with_credential(config: ServerConfig, credential: &Credential) -> Result<Self> {
        Self::from_parts(config, http_client()?, credential)
    }

    pub fn credential(&self) -> Credential {
        Credential::Navidrome {
            subsonic: self.subsonic_auth.clone(),
            nd_token: self.current_session_token(),
            password: self.saved_password(),
        }
    }

    fn from_parts(
        config: ServerConfig,
        http_client: reqwest::Client,
        credential: &Credential,
    ) -> Result<Self> {
        let Credential::Navidrome {
            subsonic,
            nd_token,
            password,
        } = credential
        else {
            return Err(ApiError::Auth("not a Navidrome credential".into()));
        };
        let base_url = trimmed_base_url(&config.url).to_owned();
        let subsonic_auth = subsonic.trim_start_matches(['?', '&']).to_owned();
        let subsonic_server = SubsonicServer::with_identity(
            config.clone(),
            subsonic_auth.clone(),
            NAVIDROME_IDENTITY,
        )?;
        let session_token = nd_token.clone().filter(|token| !token.is_empty());
        let password = password.clone().filter(|password| !password.is_empty());
        let is_session_lost = session_token.is_none() && password.is_none();
        Ok(Self {
            config,
            base_url,
            http_client,
            subsonic_auth,
            session_token: RwLock::new(session_token),
            password: RwLock::new(password),
            session_renewal: Mutex::new(()),
            session_lost: watch::Sender::new(is_session_lost),
            subsonic_server,
        })
    }

    pub(crate) fn server_id(&self) -> &str {
        &self.config.id
    }

    pub(crate) async fn session_token_or_login(&self) -> Option<String> {
        let _only_one_login_at_a_time = self.session_renewal.lock().await;
        if let Some(token) = self.current_session_token() {
            return Some(token);
        }
        let Some(password) = self.saved_password() else {
            self.report_session_state();
            return None;
        };
        match request_session(&self.http_client, &self.config, &password).await {
            Ok(auth) => {
                self.replace_session_token(Some(auth.token.clone()));
                Some(auth.token)
            }
            Err(error) => {
                tracing::warn!(%error, "renewing the Navidrome session failed");
                self.forget_password_if_rejected(&error);
                self.report_session_state();
                None
            }
        }
    }
}

fn subsonic_auth_query(username: &str, salt: String, token: String) -> String {
    query_string(&[("u", username.to_owned()), ("s", salt), ("t", token)])
}

async fn request_session(
    http_client: &reqwest::Client,
    config: &ServerConfig,
    password: &str,
) -> Result<NativeAuth> {
    let base_url = trimmed_base_url(&config.url);
    let response = http_client
        .post(format!("{base_url}/auth/login"))
        .json(&LoginRequest {
            username: &config.username,
            password,
        })
        .send()
        .await
        .map_err(ApiError::from_network)?;
    let status = response.status();
    let body = response.bytes().await.map_err(ApiError::from_network)?;
    ensure_login_accepted(status, &body)?;
    serde_json::from_slice(&body).map_err(decode_error)
}

fn ensure_login_accepted(status: StatusCode, body: &[u8]) -> Result<()> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            Err(ApiError::Auth("invalid username or password".into()))
        }
        rejected if !rejected.is_success() => Err(ApiError::Server {
            code: i32::from(rejected.as_u16()),
            message: String::from_utf8_lossy(body).trim().to_owned(),
        }),
        _ => Ok(()),
    }
}
