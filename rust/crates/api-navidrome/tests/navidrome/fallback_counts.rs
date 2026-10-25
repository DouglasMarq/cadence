use cadence_api::MusicServer;
use cadence_core::model::{AlbumListQuery, AlbumSort, Paging, SongListQuery};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer};

use crate::support::{
    AudioFormat, FALLBACK_MOCK_PRIORITY, SubsonicEntry, server, subsonic_entry, subsonic_ok,
};

#[tokio::test]
async fn song_counts_fall_back_to_subsonic_without_a_session() {
    let mock = MockServer::start().await;
    let songs: Vec<_> = ["s1", "s2", "s3"]
        .iter()
        .map(|id| {
            subsonic_entry(SubsonicEntry {
                id,
                title: id,
                parent: "al1",
                format: AudioFormat::Mp3,
            })
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/rest/search3.view"))
        .and(query_param("songOffset", "0"))
        .respond_with(subsonic_ok(json!({ "searchResult3": { "song": songs } })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/search3.view"))
        .respond_with(subsonic_ok(json!({ "searchResult3": {} })))
        .with_priority(FALLBACK_MOCK_PRIORITY)
        .mount(&mock)
        .await;

    let count = server(&mock, None)
        .song_count(&SongListQuery::default())
        .await
        .expect("count");
    assert_eq!(count, 3);
}

#[tokio::test]
async fn album_counts_fall_back_to_subsonic_without_a_session() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/getAlbumList2.view"))
        .and(query_param("type", "alphabeticalByName"))
        .and(query_param("offset", "0"))
        .respond_with(subsonic_ok(json!({ "albumList2": { "album": [
            { "id": "al1", "name": "One" },
            { "id": "al2", "name": "Two" }
        ] } })))
        .mount(&mock)
        .await;

    let query = AlbumListQuery {
        sort: AlbumSort::Random,
        ..AlbumListQuery::default()
    };
    let count = server(&mock, None)
        .album_count(&query)
        .await
        .expect("count");
    assert_eq!(count, 2);
}

#[tokio::test]
async fn given_no_session_when_searching_albums_then_the_subsonic_search_lists_them() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/search3.view"))
        .and(query_param("query", "harbor"))
        .respond_with(subsonic_ok(json!({ "searchResult3": { "album": [
            { "id": "al1", "name": "Harbor Lights" }
        ] } })))
        .expect(1)
        .mount(&mock)
        .await;
    let query = AlbumListQuery {
        search: Some("harbor".into()),
        ..AlbumListQuery::default()
    };

    let page = server(&mock, None)
        .album_list(
            &query,
            Paging {
                start_index: 0,
                limit: 10,
            },
        )
        .await
        .expect("albums");

    assert_eq!(page.items[0].name, "Harbor Lights");
}
