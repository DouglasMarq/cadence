use cadence_api::MusicServer;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{mount_ping, native_album_json, native_song_json, server, subsonic_ok};

const SEARCH_LIMIT: usize = 20;

async fn mount_native_list(mock: &MockServer, list: (&str, &str), rows: serde_json::Value) {
    let (list_path, filter) = list;
    Mock::given(method("GET"))
        .and(path(list_path))
        .and(query_param(filter, "c"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-total-count", "1")
                .set_body_json(rows),
        )
        .expect(1)
        .mount(mock)
        .await;
}

async fn mount_search3(mock: &MockServer, searches: u64) {
    Mock::given(method("GET"))
        .and(path("/rest/search3.view"))
        .respond_with(subsonic_ok(json!({ "searchResult3": {
            "song": [{ "id": "s1", "title": "Come Back Around", "parent": "al1" }]
        } })))
        .expect(searches)
        .mount(mock)
        .await;
}

#[tokio::test]
async fn given_a_one_letter_term_when_searching_then_the_native_lists_find_matches() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    let song = native_song_json("s1", "Come Back Around");
    mount_native_list(&mock, ("/api/song", "title"), json!([song])).await;
    let album = native_album_json("al1");
    mount_native_list(&mock, ("/api/album", "name"), json!([album])).await;
    let artist = json!({ "id": "ar1", "name": "Coldplay" });
    mount_native_list(&mock, ("/api/artist", "name"), json!([artist])).await;
    mount_search3(&mock, 0).await;

    let found = server(&mock, Some("jwt-1"))
        .search("c", SEARCH_LIMIT)
        .await
        .expect("search");

    assert_eq!(found.songs[0].title, "Come Back Around");
    assert_eq!(found.albums[0].id, "al1");
    assert_eq!(found.artists[0].name, "Coldplay");
}

#[tokio::test]
async fn given_a_longer_term_when_searching_then_subsonic_ranks_the_matches() {
    let mock = MockServer::start().await;
    mount_ping(&mock, "0.64.2", &[]).await;
    mount_search3(&mock, 1).await;

    let found = server(&mock, Some("jwt-1"))
        .search("come", SEARCH_LIMIT)
        .await
        .expect("search");

    assert_eq!(found.songs[0].title, "Come Back Around");
}
