use std::time::Duration;

use cadence_api::MusicServer;
use cadence_api_subsonic::SubsonicServer;
use cadence_core::model::{ArtistListQuery, ArtistPart, ArtistSort, SortOrder};
use serde_json::json;
use tokio::sync::mpsc;
use wiremock::matchers::{path, query_param};
use wiremock::{Mock, MockServer};

use crate::support::{album, artist, mount_plain_ping, ok, paging, server, song};

const NEVER_IN_THIS_TEST: Duration = Duration::from_secs(120);

async fn mount_artist_index(mock: &MockServer) {
    let mut starred = artist("ar-3", "Charlie", 7);
    starred["starred"] = json!("2024-01-01T00:00:00Z");
    Mock::given(path("/rest/getArtists.view"))
        .and(query_param("musicFolderId", "1"))
        .respond_with(ok(json!({
            "artists": {
                "ignoredArticles": "The",
                "index": [
                    { "name": "A", "artist": [artist("ar-1", "Alpha", 2), artist("ar-2", "Amber", 9)] },
                    { "name": "C", "artist": [starred] }
                ]
            }
        })))
        .mount(mock)
        .await;
}

async fn assert_sorted_by_album_count(server: &SubsonicServer) {
    let query = ArtistListQuery {
        sort: ArtistSort::AlbumCount,
        order: SortOrder::Desc,
        music_folder_id: Some("1".into()),
        ..Default::default()
    };
    let page = server
        .artist_list(&query, paging(0, 2))
        .await
        .expect("artists");
    let names: Vec<&str> = page
        .items
        .iter()
        .map(|artist| artist.name.as_str())
        .collect();
    assert_eq!(names, ["Amber", "Charlie"]);
    assert_eq!(page.total_count, Some(3));
    assert_eq!(page.items[0].album_count, Some(9));
}

async fn assert_search_and_favorites_filter(server: &SubsonicServer) {
    let search = ArtistListQuery {
        search: Some("AL".into()),
        music_folder_id: Some("1".into()),
        ..Default::default()
    };
    let page = server
        .artist_list(&search, paging(0, 10))
        .await
        .expect("search");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, "ar-1");

    let favorites = ArtistListQuery {
        is_favorite: Some(true),
        music_folder_id: Some("1".into()),
        ..Default::default()
    };
    let page = server
        .artist_list(&favorites, paging(0, 10))
        .await
        .expect("favorites");
    assert_eq!(page.items.len(), 1);
    assert!(page.items[0].is_favorite);
}

#[tokio::test]
async fn artist_list_filters_sorts_and_pages_client_side() {
    let mock = MockServer::start().await;
    mount_artist_index(&mock).await;
    let server = server(&mock);

    assert_sorted_by_album_count(&server).await;
    assert_search_and_favorites_filter(&server).await;
}

#[tokio::test]
async fn artist_detail_fetches_top_songs_and_biography() {
    let mock = MockServer::start().await;
    mount_plain_ping(&mock).await;
    Mock::given(path("/rest/getArtist.view"))
        .and(query_param("id", "ar-1"))
        .respond_with(ok(json!({
            "artist": { "id": "ar-1", "name": "The Artist", "albumCount": 1, "album": [album("a1", "One")] }
        })))
        .mount(&mock)
        .await;
    Mock::given(path("/rest/getTopSongs.view"))
        .and(query_param("artist", "The Artist"))
        .respond_with(ok(json!({ "topSongs": { "song": [song("s1", "Hit")] } })))
        .expect(1)
        .mount(&mock)
        .await;
    Mock::given(path("/rest/getArtistInfo2.view"))
        .and(query_param("id", "ar-1"))
        .respond_with(ok(json!({ "artistInfo2": { "biography": "Bio" } })))
        .mount(&mock)
        .await;

    let detail = server(&mock).artist("ar-1").await.expect("artist");
    assert_eq!(detail.artist.name, "The Artist");
    assert_eq!(detail.artist.biography.as_deref(), Some("Bio"));
    assert_eq!(detail.albums.len(), 1);
    assert_eq!(detail.top_songs[0].title, "Hit");
}

#[tokio::test]
async fn given_a_slow_biography_and_top_songs_when_delivered_in_parts_then_the_profile_does_not_wait()
 {
    let mock = MockServer::start().await;
    mount_plain_ping(&mock).await;
    Mock::given(path("/rest/getArtist.view"))
        .respond_with(ok(json!({
            "artist": { "id": "ar-1", "name": "The Artist", "albumCount": 1, "album": [album("a1", "One")] }
        })))
        .mount(&mock)
        .await;
    for slow in ["/rest/getTopSongs.view", "/rest/getArtistInfo2.view"] {
        Mock::given(path(slow))
            .respond_with(ok(json!({})).set_delay(NEVER_IN_THIS_TEST))
            .mount(&mock)
            .await;
    }
    let server = server(&mock);
    let (sender, mut parts) = mpsc::unbounded_channel();
    let deliver = move |part: ArtistPart| {
        sender.send(part).expect("the test is listening");
    };

    let early_parts = async { (parts.recv().await, parts.recv().await) };
    let (profile, albums) = tokio::select! {
        early_parts = early_parts => early_parts,
        _finished = server.artist_in_parts("ar-1", &deliver) => panic!("slow calls cannot finish"),
    };

    assert!(matches!(profile, Some(ArtistPart::Profile(artist)) if artist.name == "The Artist"));
    assert!(matches!(albums, Some(ArtistPart::Albums(albums)) if albums.len() == 1));
}

#[tokio::test]
async fn given_a_full_answer_when_delivered_in_parts_then_every_section_arrives() {
    let mock = MockServer::start().await;
    mount_plain_ping(&mock).await;
    Mock::given(path("/rest/getArtist.view"))
        .respond_with(ok(json!({
            "artist": { "id": "ar-1", "name": "The Artist", "albumCount": 1, "album": [album("a1", "One")] }
        })))
        .mount(&mock)
        .await;
    Mock::given(path("/rest/getTopSongs.view"))
        .respond_with(ok(json!({ "topSongs": { "song": [song("s1", "Hit")] } })))
        .mount(&mock)
        .await;
    Mock::given(path("/rest/getArtistInfo2.view"))
        .respond_with(ok(json!({ "artistInfo2": { "biography": "Life story" } })))
        .mount(&mock)
        .await;
    let server = server(&mock);
    let (sender, mut parts) = mpsc::unbounded_channel();
    let deliver = move |part: ArtistPart| {
        sender.send(part).expect("the test is listening");
    };

    server
        .artist_in_parts("ar-1", &deliver)
        .await
        .expect("the artist answers");

    let mut detail = None;
    while let Ok(part) = parts.try_recv() {
        part.merge_into(&mut detail);
    }
    let detail = detail.expect("the profile arrived");
    assert_eq!(detail.albums.len(), 1);
    assert_eq!(detail.top_songs.len(), 1);
    assert_eq!(detail.artist.biography.as_deref(), Some("Life story"));
}
