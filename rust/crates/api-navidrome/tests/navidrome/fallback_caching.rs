use cadence_api::MusicServer;
use cadence_api_navidrome::NavidromeServer;
use cadence_core::model::{ArtistListQuery, ItemKind, Paging, SortOrder};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

use crate::support::{server, subsonic_ok};

const ARTIST_PAGE_SIZE: usize = 2;

fn artist_index() -> Value {
    let artists: Vec<Value> = (0..10)
        .map(|index| json!({ "id": format!("ar{index}"), "name": format!("Artist {index}") }))
        .collect();
    json!({ "artists": { "index": [{ "name": "A", "artist": artists }] } })
}

fn subsonic_mock(endpoint: &str, body: Value, expected_calls: u64) -> Mock {
    Mock::given(method("GET"))
        .and(path(format!("/rest/{endpoint}.view")))
        .respond_with(subsonic_ok(body))
        .expect(expected_calls)
}

async fn artist_page_ids(
    server: &NavidromeServer,
    page_number: usize,
    order: SortOrder,
) -> Vec<String> {
    let query = ArtistListQuery {
        order,
        ..ArtistListQuery::default()
    };
    let paging = Paging {
        start_index: page_number * ARTIST_PAGE_SIZE,
        limit: ARTIST_PAGE_SIZE,
    };
    server
        .artist_list(&query, paging)
        .await
        .map(|page| page.items.into_iter().map(|artist| artist.id).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn fallback_artist_pages_fetch_the_index_once() {
    let mock = MockServer::start().await;
    subsonic_mock("getArtists", artist_index(), 1)
        .mount(&mock)
        .await;
    let server = server(&mock, None);

    for page_number in 0..5 {
        let ids = artist_page_ids(&server, page_number, SortOrder::Asc).await;
        assert_eq!(ids.first(), Some(&format!("ar{}", page_number * 2)));
    }
    assert_eq!(
        artist_page_ids(&server, 0, SortOrder::Desc).await,
        ["ar9", "ar8"]
    );
}

#[tokio::test]
async fn fallback_artist_index_refreshes_after_a_favorite_change() {
    let mock = MockServer::start().await;
    subsonic_mock("getArtists", artist_index(), 2)
        .mount(&mock)
        .await;
    subsonic_mock("star", json!({}), 1).mount(&mock).await;
    let server = server(&mock, None);

    artist_page_ids(&server, 0, SortOrder::Asc).await;
    artist_page_ids(&server, 1, SortOrder::Asc).await;
    assert!(
        server
            .set_favorite(ItemKind::Artist, &["ar1".into()], true)
            .await
            .is_ok()
    );
    artist_page_ids(&server, 2, SortOrder::Asc).await;
}

#[tokio::test]
async fn fallback_playlist_pages_are_cached_until_a_playlist_is_deleted() {
    let mock = MockServer::start().await;
    let playlists = json!({ "playlists": { "playlist": [
        { "id": "p1", "name": "One" }, { "id": "p2", "name": "Two" }
    ] } });
    subsonic_mock("getPlaylists", playlists, 2)
        .mount(&mock)
        .await;
    subsonic_mock("deletePlaylist", json!({}), 1)
        .mount(&mock)
        .await;
    let server = server(&mock, None);
    let first = Paging {
        start_index: 0,
        limit: 1,
    };
    let second = Paging {
        start_index: 1,
        limit: 1,
    };

    assert!(server.playlist_list(first).await.is_ok());
    assert!(server.playlist_list(second).await.is_ok());
    assert!(server.delete_playlist("p2").await.is_ok());
    assert!(server.playlist_list(first).await.is_ok());
}

#[tokio::test]
async fn given_no_session_when_searching_artists_then_the_subsonic_index_is_filtered() {
    let mock = MockServer::start().await;
    subsonic_mock("getArtists", artist_index(), 1)
        .mount(&mock)
        .await;
    let query = ArtistListQuery {
        search: Some("artist 7".into()),
        ..ArtistListQuery::default()
    };

    let page = server(&mock, None)
        .artist_list(
            &query,
            Paging {
                start_index: 0,
                limit: 0,
            },
        )
        .await
        .expect("artists");

    let ids: Vec<String> = page.items.into_iter().map(|artist| artist.id).collect();
    assert_eq!(ids, ["ar7"]);
}
