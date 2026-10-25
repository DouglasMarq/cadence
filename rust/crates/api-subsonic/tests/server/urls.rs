use cadence_api::{Credential, MusicServer, StreamOptions, TranscodeFormat};
use cadence_api_subsonic::{ClientIdentity, SubsonicServer};

use crate::support::{CREDENTIAL, config};

#[test]
fn url_builders() {
    let credential = Credential::Subsonic {
        subsonic: CREDENTIAL.into(),
    };
    let server = SubsonicServer::with_credential(config("https://music.example.com/"), &credential)
        .expect("server");

    assert_eq!(
        server.stream_url("a b/c", &StreamOptions::default()),
        format!(
            "https://music.example.com/rest/stream.view?id=a%20b%2Fc&v=1.13.0&c=Cadence&{CREDENTIAL}"
        )
    );
    let options = StreamOptions {
        max_bitrate_kbps: Some(192),
        format: TranscodeFormat::parse("opus"),
    };
    assert_eq!(
        server.stream_url("a b/c", &options),
        format!(
            "https://music.example.com/rest/stream.view?id=a%20b%2Fc&v=1.13.0&c=Cadence&{CREDENTIAL}&format=opus&maxBitRate=192"
        )
    );
    assert_eq!(
        server.cover_art_url("al-1", Some(300)),
        format!(
            "https://music.example.com/rest/getCoverArt.view?id=al-1&{CREDENTIAL}&v=1.13.0&c=Cadence&size=300"
        )
    );
    assert_eq!(
        server.cover_art_url("al-1", None),
        format!(
            "https://music.example.com/rest/getCoverArt.view?id=al-1&{CREDENTIAL}&v=1.13.0&c=Cadence"
        )
    );
}

#[test]
fn given_an_address_with_surrounding_spaces_when_building_a_stream_url_then_they_are_trimmed() {
    let credential = Credential::Subsonic {
        subsonic: CREDENTIAL.into(),
    };
    let server =
        SubsonicServer::with_credential(config(" https://music.example.com/ "), &credential)
            .expect("server");
    assert_eq!(
        server.stream_url("s1", &StreamOptions::default()),
        format!("https://music.example.com/rest/stream.view?id=s1&v=1.13.0&c=Cadence&{CREDENTIAL}")
    );
}

#[test]
fn given_a_client_identity_when_building_a_stream_url_then_it_names_that_client() {
    let identity = ClientIdentity {
        api_version: "1.16.1",
        client_name: "cadence",
    };
    let server = SubsonicServer::with_identity(
        config("https://music.example.com/"),
        CREDENTIAL.into(),
        identity,
    )
    .expect("server");
    assert_eq!(
        server.stream_url("s1", &StreamOptions::default()),
        format!("https://music.example.com/rest/stream.view?id=s1&v=1.16.1&c=cadence&{CREDENTIAL}")
    );
}
