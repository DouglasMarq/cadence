mod credential;
mod envelope;
mod params;
mod payload;

use cadence_api::{ApiError, Result, http_client};
use cadence_core::web_url::trimmed_base_url;
use reqwest::{RequestBuilder, Response};

pub use credential::{
    encode_uri_component, password_credential, token_credential,
    validate_token_or_password_credential,
};
pub use params::Params;
pub use payload::Payload;

use envelope::unwrap_subsonic_response;

pub const SUBSONIC_IDENTITY: ClientIdentity = ClientIdentity {
    api_version: "1.13.0",
    client_name: "Cadence",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIdentity {
    pub api_version: &'static str,
    pub client_name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestEncoding {
    QueryString,
    FormPost,
}

pub struct Client {
    http: reqwest::Client,
    base: String,
    auth: String,
    identity: ClientIdentity,
}

fn total_count_header(response: &Response) -> Option<usize> {
    response
        .headers()
        .get("x-total-count")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.trim().parse().ok())
}

impl Client {
    pub fn new(base: &str, auth: String, identity: ClientIdentity) -> Result<Self> {
        Ok(Self {
            http: http_client()?,
            base: trimmed_base_url(base).to_owned(),
            auth,
            identity,
        })
    }

    pub fn identity(&self) -> ClientIdentity {
        self.identity
    }

    pub fn auth(&self) -> &str {
        &self.auth
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn query(&self, params: &Params) -> String {
        let ClientIdentity {
            api_version,
            client_name,
        } = self.identity;
        let mut query = format!("{}&v={api_version}&c={client_name}&f=json", self.auth);
        params.append_encoded_to(&mut query);
        query
    }

    fn build_request(
        &self,
        url: String,
        query: String,
        encoding: RequestEncoding,
    ) -> RequestBuilder {
        match encoding {
            RequestEncoding::FormPost => self
                .http
                .post(url)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(query),
            RequestEncoding::QueryString => self.http.get(format!("{url}?{query}")),
        }
    }

    pub async fn call(
        &self,
        endpoint: &str,
        params: &Params,
        encoding: RequestEncoding,
    ) -> Result<Payload> {
        let url = format!("{}/rest/{endpoint}", self.base);
        let request = self.build_request(url, self.query(params), encoding);
        let response = request.send().await.map_err(ApiError::from_network)?;
        let http_status = response.status();
        let total_count = total_count_header(&response);
        let text = response.text().await.map_err(ApiError::from_network)?;
        let envelope = unwrap_subsonic_response(http_status, &text)?;
        Ok(Payload::new(endpoint, envelope, total_count))
    }
}
