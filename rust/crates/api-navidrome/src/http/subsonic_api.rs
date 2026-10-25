use cadence_api_subsonic::ClientIdentity;
use url::form_urlencoded;

use crate::NavidromeServer;

pub(crate) const NAVIDROME_IDENTITY: ClientIdentity = ClientIdentity {
    api_version: "1.16.1",
    client_name: "cadence",
};

pub(crate) fn query_string(params: &[(&str, String)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

impl NavidromeServer {
    pub(crate) fn subsonic_url(&self, endpoint: &str, params: &[(&str, String)]) -> String {
        format!(
            "{}/rest/{endpoint}.view?{}",
            self.base_url,
            self.subsonic_query(params)
        )
    }

    fn subsonic_query(&self, params: &[(&str, String)]) -> String {
        let mut common = vec![
            ("v", NAVIDROME_IDENTITY.api_version.to_owned()),
            ("c", NAVIDROME_IDENTITY.client_name.to_owned()),
        ];
        common.extend(params.iter().map(|(key, value)| (*key, value.clone())));
        let query = query_string(&common);
        match self.subsonic_auth.as_str() {
            "" => query,
            auth => format!("{auth}&{query}"),
        }
    }
}
