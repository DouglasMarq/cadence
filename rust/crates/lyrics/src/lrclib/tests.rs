use super::*;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn query() -> LyricsQuery {
    LyricsQuery {
        title: "Lantern Harbor".to_owned(),
        artist: "Velvet Comets".to_owned(),
        album: None,
        duration_ms: Some(354_000),
    }
}

struct TrackFixture<'name> {
    id: u64,
    name: &'name str,
    duration: f64,
    plain: Value,
    synced: Value,
}

fn track(fixture: TrackFixture) -> Value {
    json!({
        "id": fixture.id,
        "name": fixture.name,
        "trackName": fixture.name,
        "artistName": "Velvet Comets",
        "albumName": "Lights Over the Bay",
        "duration": fixture.duration,
        "instrumental": false,
        "plainLyrics": fixture.plain,
        "syncedLyrics": fixture.synced
    })
}

async fn mount_get(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/api/get"))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount_search(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("q", "Lantern Harbor Velvet Comets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_json(json!({
        "code": 404, "name": "TrackNotFound", "message": "Failed to find specified track"
    }))
}

#[tokio::test]
async fn get_endpoint_sends_track_signature() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/get"))
        .and(query_param("track_name", "Lantern Harbor"))
        .and(query_param("artist_name", "Velvet Comets"))
        .and(query_param("album_name", "Lantern Harbor"))
        .and(query_param("duration", "354"))
        .and(header("user-agent", AGENT))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(track(TrackFixture {
                id: 1,
                name: "Lantern Harbor",
                duration: 354.0,
                plain: json!("Do the lanterns still glow?"),
                synced: json!(
                    "[00:00.50] Do the lanterns still glow?\n[00:04.10] Do they only flicker?"
                ),
            })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let lyrics = fetch(&reqwest::Client::new(), &server.uri(), &query())
        .await
        .unwrap()
        .unwrap();
    assert!(lyrics.is_synced);
    assert_eq!(lyrics.source, LyricsSource::Lrclib);
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[1].time_ms, Some(4100));
    assert_eq!(lyrics.lines[1].text, "Do they only flicker?");
}

#[tokio::test]
async fn plain_get_result_prefers_synced_search_hit() {
    let server = MockServer::start().await;
    mount_get(
        &server,
        ResponseTemplate::new(200).set_body_json(track(TrackFixture {
            id: 1,
            name: "Lantern Harbor",
            duration: 354.0,
            plain: json!("Plain only"),
            synced: Value::Null,
        })),
    )
    .await;
    mount_search(
        &server,
        json!([
            track(TrackFixture {
                id: 2,
                name: "Lantern Harbor",
                duration: 354.5,
                plain: json!("Plain"),
                synced: Value::Null
            }),
            track(TrackFixture {
                id: 3,
                name: "Lantern Harbor",
                duration: 355.0,
                plain: json!("Synced plain"),
                synced: json!("[00:01.00] Synced")
            })
        ]),
    )
    .await;

    let lyrics = fetch(&reqwest::Client::new(), &server.uri(), &query())
        .await
        .unwrap()
        .unwrap();
    assert!(lyrics.is_synced);
    assert_eq!(lyrics.lines[0].text, "Synced");
}

#[tokio::test]
async fn keeps_plain_get_result_when_search_has_nothing_synced() {
    let server = MockServer::start().await;
    mount_get(
        &server,
        ResponseTemplate::new(200).set_body_json(track(TrackFixture {
            id: 1,
            name: "Lantern Harbor",
            duration: 354.0,
            plain: json!("Line one\nLine two"),
            synced: Value::Null,
        })),
    )
    .await;
    mount_search(&server, json!([])).await;

    let lyrics = fetch(&reqwest::Client::new(), &server.uri(), &query())
        .await
        .unwrap()
        .unwrap();
    assert!(!lyrics.is_synced);
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[0].time_ms, None);
}

#[tokio::test]
async fn search_skips_wrong_duration_and_poor_matches() {
    let server = MockServer::start().await;
    mount_get(&server, not_found()).await;
    mount_search(
        &server,
        json!([
            track(TrackFixture {
                id: 2,
                name: "Lantern Harbor",
                duration: 300.0,
                plain: Value::Null,
                synced: json!("[00:01.00] wrong length")
            }),
            track(TrackFixture {
                id: 3,
                name: "Totally Unrelated Tune",
                duration: 354.0,
                plain: Value::Null,
                synced: json!("[00:01.00] unrelated")
            }),
            track(TrackFixture {
                id: 4,
                name: "Lantern Harbor (Remastered 2011)",
                duration: 355.5,
                plain: json!("right"),
                synced: json!("[00:02.00] right")
            })
        ]),
    )
    .await;

    let lyrics = fetch(&reqwest::Client::new(), &server.uri(), &query())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lyrics.lines[0].text, "right");
    assert_eq!(lyrics.lines[0].time_ms, Some(2000));
}

#[tokio::test]
async fn instrumental_and_missing_tracks_yield_none() {
    let server = MockServer::start().await;
    mount_get(&server, not_found()).await;
    mount_search(
        &server,
        json!([track(TrackFixture {
            id: 2,
            name: "Lantern Harbor",
            duration: 354.0,
            plain: Value::Null,
            synced: Value::Null
        })]),
    )
    .await;

    let lyrics = fetch(&reqwest::Client::new(), &server.uri(), &query())
        .await
        .unwrap();
    assert!(lyrics.is_none());
}

#[tokio::test]
async fn malformed_json_is_a_decode_error() {
    let server = MockServer::start().await;
    mount_get(
        &server,
        ResponseTemplate::new(200).set_body_string("<html>not json</html>"),
    )
    .await;
    let result = fetch(&reqwest::Client::new(), &server.uri(), &query()).await;
    let Err(error) = result else {
        panic!("an html page is not lyrics json");
    };
    assert!(matches!(error, crate::LyricsError::Decode(_)));
    let source = std::error::Error::source(&error).unwrap();
    assert!(source.downcast_ref::<serde_json::Error>().is_some());
}
