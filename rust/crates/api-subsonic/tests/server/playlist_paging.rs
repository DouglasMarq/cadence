use cadence_api::MusicServer;
use serde_json::{Value, json};
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer};

use crate::support::{ok, paging, playlist, server, song};

fn playlist_with_songs(id: &str, count: usize) -> Value {
    let mut detail = playlist(id, "Mix");
    let entries: Vec<Value> = (0..count)
        .map(|index| song(&format!("s{index}"), &format!("Song {index}")))
        .collect();
    detail["songCount"] = json!(count);
    detail["entry"] = json!(entries);
    detail
}

fn playlist_mock(id: &str, count: usize, expected_fetches: u64) -> Mock {
    Mock::given(path("/rest/getPlaylist.view"))
        .and(query_param("id", id))
        .respond_with(ok(json!({ "playlist": playlist_with_songs(id, count) })))
        .expect(expected_fetches)
}

fn page_ids(page: &cadence_core::model::Page<cadence_core::model::Song>) -> Vec<&str> {
    page.items.iter().map(|song| song.id.as_str()).collect()
}

#[tokio::test]
async fn playlist_pages_come_from_one_cached_fetch() {
    let mock = MockServer::start().await;
    playlist_mock("p1", 250, 1).mount(&mock).await;
    let server = server(&mock);

    let header = server.playlist("p1").await.expect("playlist");
    assert_eq!(header.song_count, 250);
    assert_eq!(header.duration_ms, 360_000);
    let first = server
        .playlist_song_page("p1", paging(0, 100))
        .await
        .expect("first");
    assert_eq!(first.total_count, Some(250));
    assert_eq!(first.items.len(), 100);
    let last = server
        .playlist_song_page("p1", paging(200, 100))
        .await
        .expect("last");
    assert_eq!(last.start_index, 200);
    assert_eq!(page_ids(&last).first().copied(), Some("s200"));
    assert_eq!(last.items.len(), 50);
    let all = server.playlist_songs("p1").await.expect("all");
    assert_eq!(all.len(), 250);
}

#[tokio::test]
async fn playlist_edits_invalidate_the_cached_copy() {
    let mock = MockServer::start().await;
    playlist_mock("p1", 3, 4).mount(&mock).await;
    Mock::given(path("/rest/updatePlaylist.view"))
        .respond_with(ok(json!({})))
        .expect(2)
        .mount(&mock)
        .await;
    Mock::given(path("/rest/deletePlaylist.view"))
        .respond_with(ok(json!({})))
        .expect(1)
        .mount(&mock)
        .await;
    let server = server(&mock);

    server
        .playlist_song_page("p1", paging(0, 2))
        .await
        .expect("page");
    server
        .remove_from_playlist("p1", &[0])
        .await
        .expect("remove");
    server
        .playlist_song_page("p1", paging(0, 2))
        .await
        .expect("page");
    server
        .add_to_playlist("p1", &["s9".into()])
        .await
        .expect("add");
    server
        .playlist_song_page("p1", paging(0, 2))
        .await
        .expect("page");
    server.delete_playlist("p1").await.expect("delete");
    server.playlist("p1").await.expect("playlist");
}

#[tokio::test]
async fn playlists_are_cached_per_id() {
    let mock = MockServer::start().await;
    playlist_mock("p1", 2, 1).mount(&mock).await;
    playlist_mock("p2", 4, 1).mount(&mock).await;
    let server = server(&mock);

    let first = server
        .playlist_song_page("p1", paging(0, 10))
        .await
        .expect("p1");
    let second = server
        .playlist_song_page("p2", paging(0, 10))
        .await
        .expect("p2");
    assert_eq!(first.total_count, Some(2));
    assert_eq!(second.total_count, Some(4));
    server
        .playlist_song_page("p1", paging(1, 10))
        .await
        .expect("p1 again");
    server
        .playlist_song_page("p2", paging(3, 10))
        .await
        .expect("p2 again");
}
