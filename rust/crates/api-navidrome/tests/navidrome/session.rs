use cadence_api::{ApiError, Credential, MusicServer};
use cadence_api_navidrome::NavidromeServer;
use cadence_core::model::{Paging, SongListQuery};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    SUBSONIC_AUTH, config, mount_ping, native_song_json, server, subsonic_failed, subsonic_ok,
};

async fn mount_login(mock: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .and(body_json(
            json!({ "username": "alice", "password": "secret" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "u1",
            "isAdmin": false,
            "name": "Alice",
            "subsonicSalt": "salt1",
            "subsonicToken": "tok1",
            "token": "jwt-1",
            "username": "alice"
        })))
        .expect(1)
        .mount(mock)
        .await;
}

#[tokio::test]
async fn login_builds_navidrome_credential() {
    let mock = MockServer::start().await;
    mount_login(&mock).await;

    let (server, credential) = NavidromeServer::login(config(&mock.uri()), "secret")
        .await
        .expect("login");

    assert_eq!(
        credential,
        Credential::Navidrome {
            subsonic: SUBSONIC_AUTH.into(),
            nd_token: Some("jwt-1".into()),
            password: Some("secret".into()),
        }
    );
    assert_eq!(server.credential(), credential);
    assert_eq!(server.config().id, "srv1");
}

#[tokio::test]
async fn login_rejected_is_auth_error() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "error": "Invalid" })))
        .mount(&mock)
        .await;

    let result = NavidromeServer::login(config(&mock.uri()), "wrong").await;
    assert!(matches!(result, Err(ApiError::Auth(_))));
}

#[tokio::test]
async fn with_credential_rejects_other_backends() {
    let result = NavidromeServer::with_credential(
        config("http://localhost:1"),
        &Credential::Subsonic {
            subsonic: SUBSONIC_AUTH.into(),
        },
    );
    assert!(matches!(result, Err(ApiError::Auth(_))));
}

#[tokio::test]
async fn native_calls_pick_up_refreshed_token() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/song/s1"))
        .and(header("x-nd-authorization", "Bearer jwt-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-nd-authorization", "jwt-2")
                .set_body_json(native_song_json("s1", "First")),
        )
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/song/s2"))
        .and(header("x-nd-authorization", "Bearer jwt-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(native_song_json("s2", "Second")))
        .expect(1)
        .mount(&mock)
        .await;

    let server = server(&mock, Some("jwt-1"));
    assert_eq!(server.song("s1").await.expect("s1").title, "First");
    assert_eq!(
        server.credential(),
        Credential::Navidrome {
            subsonic: SUBSONIC_AUTH.into(),
            nd_token: Some("jwt-2".into()),
            password: None,
        }
    );
    assert_eq!(server.song("s2").await.expect("s2").title, "Second");
}

fn subsonic_song_fixture() -> Value {
    json!({
        "id": "s1",
        "parent": "al1",
        "isDir": false,
        "title": "From Subsonic",
        "album": "Album One",
        "artist": "Artist One",
        "artistId": "ar1",
        "albumId": "al1",
        "coverArt": "mf-s1",
        "size": 100,
        "contentType": "audio/mpeg",
        "suffix": "mp3",
        "duration": 200,
        "path": "a/b.mp3",
        "created": "2024-01-01T00:00:00Z",
        "type": "music",
        "isVideo": false,
        "discNumber": 1,
        "track": 3,
        "artists": [{ "id": "ar1", "name": "Artist One" }],
        "albumArtists": [{ "id": "ar1", "name": "Artist One" }]
    })
}

async fn mount_expired_native_and_subsonic_song(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/song/s1"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/getSong.view"))
        .and(query_param("id", "s1"))
        .and(query_param("u", "alice"))
        .and(query_param("s", "salt1"))
        .and(query_param("t", "tok1"))
        .and(query_param("v", "1.16.1"))
        .and(query_param("c", "cadence"))
        .and(query_param("f", "json"))
        .respond_with(subsonic_ok(json!({ "song": subsonic_song_fixture() })))
        .expect(2)
        .mount(mock)
        .await;
}

#[tokio::test]
async fn rejected_token_falls_back_to_subsonic() {
    let mock = MockServer::start().await;
    mount_expired_native_and_subsonic_song(&mock).await;

    let server = server(&mock, Some("expired"));
    let song = server.song("s1").await.expect("fallback song");
    assert_eq!(song.title, "From Subsonic");
    assert_eq!(song.cover_art_id.as_deref(), Some("mf-s1"));
    assert_eq!(song.track_number, Some(3));
    assert_eq!(
        server.credential(),
        Credential::Navidrome {
            subsonic: SUBSONIC_AUTH.into(),
            nd_token: None,
            password: None,
        }
    );
    server
        .song("s1")
        .await
        .expect("second call goes straight to subsonic");
}

#[tokio::test]
async fn subsonic_errors_are_mapped() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/scrobble.view"))
        .respond_with(subsonic_failed(70, "not found"))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/ping.view"))
        .respond_with(subsonic_failed(40, "Wrong username or password"))
        .mount(&mock)
        .await;

    let server = server(&mock, None);
    assert!(matches!(
        server.scrobble("x", true, None).await,
        Err(ApiError::NotFound)
    ));
    assert!(matches!(server.ping().await, Err(ApiError::Auth(_))));
}

#[tokio::test]
async fn given_a_network_failure_the_error_has_no_credentials() {
    let unreachable = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port");
    let server = NavidromeServer::with_credential(
        config(&format!("http://{unreachable}")),
        &Credential::Navidrome {
            subsonic: SUBSONIC_AUTH.into(),
            nd_token: None,
            password: None,
        },
    )
    .expect("server");

    let error = server.ping().await.expect_err("unreachable server");

    assert!(matches!(error, ApiError::Network(_)));
    let message = error.to_string();
    assert!(
        !message.contains("salt1") && !message.contains("tok1"),
        "{message}"
    );
    let source = std::error::Error::source(&error).expect("the transport error");
    assert!(source.downcast_ref::<reqwest::Error>().is_some());
}

fn saved_login(token: Option<&str>) -> Credential {
    Credential::Navidrome {
        subsonic: SUBSONIC_AUTH.into(),
        nd_token: token.map(str::to_owned),
        password: Some("secret".into()),
    }
}

fn server_with_saved_password(mock: &MockServer, token: Option<&str>) -> NavidromeServer {
    NavidromeServer::with_credential(config(&mock.uri()), &saved_login(token)).expect("server")
}

fn renewed_login() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "subsonicSalt": "salt2",
        "subsonicToken": "tok2",
        "token": "jwt-renewed",
    }))
}

async fn mount_renewal(mock: &MockServer, response: ResponseTemplate, logins: u64) {
    Mock::given(method("POST"))
        .and(path("/auth/login"))
        .and(body_json(
            json!({ "username": "alice", "password": "secret" }),
        ))
        .respond_with(response)
        .expect(logins)
        .mount(mock)
        .await;
}

async fn mount_sorted_songs_for_renewed_session(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/song"))
        .and(query_param("_sort", "title"))
        .and(query_param("_order", "ASC"))
        .and(header("x-nd-authorization", "Bearer jwt-renewed"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-count", "1")
                .set_body_json(json!([native_song_json("s1", "Black Out Days")])),
        )
        .mount(mock)
        .await;
}

async fn mount_expired_song_list(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/song"))
        .and(header("x-nd-authorization", "Bearer expired"))
        .respond_with(ResponseTemplate::new(401))
        .mount(mock)
        .await;
}

async fn mount_subsonic_song_search(mock: &MockServer, searches: u64) {
    Mock::given(method("GET"))
        .and(path("/rest/search3.view"))
        .respond_with(subsonic_ok(json!({ "searchResult3": {} })))
        .expect(searches)
        .mount(mock)
        .await;
}

fn first_songs() -> Paging {
    Paging {
        start_index: 0,
        limit: 50,
    }
}

#[tokio::test]
async fn given_an_expired_session_and_a_saved_password_when_listing_songs_then_it_logs_in_again() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_expired_song_list(&mock).await;
    mount_renewal(&mock, renewed_login(), 1).await;
    mount_sorted_songs_for_renewed_session(&mock).await;
    mount_subsonic_song_search(&mock, 0).await;
    let server = server_with_saved_password(&mock, Some("expired"));

    let page = server
        .song_list(&SongListQuery::default(), first_songs())
        .await
        .expect("sorted songs");

    assert_eq!(page.items[0].title, "Black Out Days");
    assert_eq!(server.credential(), saved_login(Some("jwt-renewed")));
}

#[tokio::test]
async fn given_a_saved_password_without_a_session_when_listing_songs_then_it_logs_in_first() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_renewal(&mock, renewed_login(), 1).await;
    mount_sorted_songs_for_renewed_session(&mock).await;
    mount_subsonic_song_search(&mock, 0).await;
    let server = server_with_saved_password(&mock, None);

    let page = server
        .song_list(&SongListQuery::default(), first_songs())
        .await
        .expect("sorted songs");

    assert_eq!(page.items[0].title, "Black Out Days");
}

#[tokio::test]
async fn given_many_requests_with_an_expired_session_when_they_fail_together_then_it_logs_in_once()
{
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_expired_song_list(&mock).await;
    mount_renewal(&mock, renewed_login(), 1).await;
    mount_sorted_songs_for_renewed_session(&mock).await;
    let server = server_with_saved_password(&mock, Some("expired"));
    let query = SongListQuery::default();

    let (first, second, third) = tokio::join!(
        server.song_list(&query, first_songs()),
        server.song_count(&query),
        server.song_list(&query, first_songs()),
    );

    assert_eq!(first.expect("first list").items.len(), 1);
    assert_eq!(second.expect("count"), 1);
    assert_eq!(third.expect("second list").items.len(), 1);
}

#[tokio::test]
async fn given_a_changed_password_when_renewing_then_it_stops_trying_and_uses_subsonic() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_expired_song_list(&mock).await;
    mount_renewal(&mock, ResponseTemplate::new(401), 1).await;
    mount_subsonic_song_search(&mock, 2).await;
    let server = server_with_saved_password(&mock, Some("expired"));

    for _ in 0..2 {
        server
            .song_list(&SongListQuery::default(), first_songs())
            .await
            .expect("songs from subsonic");
    }

    assert_eq!(
        server.credential(),
        Credential::Navidrome {
            subsonic: SUBSONIC_AUTH.into(),
            nd_token: None,
            password: None,
        }
    );
}

#[tokio::test]
async fn given_a_server_failure_when_renewing_then_the_password_is_kept_for_the_next_request() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_renewal(&mock, ResponseTemplate::new(500), 1).await;
    mount_subsonic_song_search(&mock, 1).await;
    let server = server_with_saved_password(&mock, None);

    server
        .song_list(&SongListQuery::default(), first_songs())
        .await
        .expect("songs from subsonic");

    assert_eq!(server.credential(), saved_login(None));
}

#[tokio::test]
async fn given_a_login_saved_by_an_older_version_when_it_is_read_then_it_has_no_password() {
    let saved = r#"{"type":"navidrome","subsonic":"u=alice","nd_token":"jwt-1"}"#;

    let credential = Credential::from_json(saved).expect("older login");

    assert_eq!(
        credential,
        Credential::Navidrome {
            subsonic: "u=alice".into(),
            nd_token: Some("jwt-1".into()),
            password: None,
        }
    );
}
