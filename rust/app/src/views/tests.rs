use std::future::pending;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cadence_api::MusicServer;
use cadence_core::model::{Album, Artist, ArtistDetail, ArtistPart, ItemKind, ServerConfig, Song};
use cadence_storage::cover_cache::DecodedImage;
use tempfile::TempDir;
use tokio::sync::oneshot;

use super::{FocusedCovers, View, ViewSlot, ViewStore, Views};
use crate::covers::{CoverProvider, DemoCovers};
use crate::demo::{DemoLibrary, FakeServer, test_song};
use crate::session::Session;
use crate::tests::refusing_server::RefusingServer;

const FIRST_SERVER: &str = "server-one";
const SECOND_SERVER: &str = "server-two";

fn demo_on(server_id: &str) -> Arc<dyn MusicServer> {
    let config = ServerConfig {
        id: server_id.to_owned(),
        ..FakeServer::config_for_demo()
    };
    Arc::new(FakeServer::with_config(config, DemoLibrary::with_songs(20)))
}

fn views_on(server: Arc<dyn MusicServer>, store: &Arc<ViewStore>, session: &Session) -> Views {
    Views::new(
        server,
        Arc::clone(store),
        session.clone(),
        tokio::runtime::Handle::current(),
    )
}

fn store_in(dir: &TempDir) -> Arc<ViewStore> {
    Arc::new(ViewStore::new(dir.path()))
}

fn scratch_dir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn slot() -> ViewSlot {
    View::Albums.slot("page-0")
}

type Delivered<Payload> = Arc<Mutex<Vec<Result<Payload, String>>>>;

fn recorder<Payload>() -> Delivered<Payload> {
    Arc::default()
}

fn recording<Payload: Send + 'static>(
    delivered: &Delivered<Payload>,
) -> impl Fn(Result<Payload, String>) + Send + Sync + 'static {
    let delivered = Arc::clone(delivered);
    move |result| delivered.lock().unwrap().push(result)
}

async fn settle_until(is_done: impl Fn() -> bool) {
    for _attempt in 0..2_000 {
        if is_done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the condition never became true");
}

fn count_of<Payload>(delivered: &Delivered<Payload>) -> usize {
    delivered.lock().unwrap().len()
}

async fn answered(value: usize) -> Result<usize, String> {
    Ok(value)
}

#[tokio::test]
async fn given_a_cached_view_when_loaded_then_it_appears_before_the_server_answers() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store.seed(FIRST_SERVER, &slot(), 7_usize).await;
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let delivered = recorder();
    let (answer, answer_received) = oneshot::channel::<usize>();

    views.load(
        Some(slot()),
        async move { Ok(answer_received.await.unwrap()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;
    assert_eq!(delivered.lock().unwrap().as_slice(), [Ok(7)]);

    answer.send(9).unwrap();
    settle_until(|| count_of(&delivered) == 2).await;
    assert_eq!(delivered.lock().unwrap().as_slice(), [Ok(7), Ok(9)]);
}

#[tokio::test]
async fn given_a_refresh_when_a_later_run_opens_the_view_then_it_reads_the_refreshed_answer() {
    let dir = scratch_dir();
    let session = Session::default();
    let first_run = store_in(&dir);
    let views = views_on(demo_on(FIRST_SERVER), &first_run, &session);
    let delivered = recorder();
    views.load(Some(slot()), answered(3), recording(&delivered));
    settle_until(|| count_of(&delivered) == 1).await;
    settle_until_stored(&first_run, 3).await;

    let restarted = store_in(&dir);

    assert_eq!(
        restarted.stored::<usize>(FIRST_SERVER, &slot()).await,
        Some(3)
    );
}

async fn settle_until_stored(store: &Arc<ViewStore>, expected: usize) {
    for _attempt in 0..2_000 {
        if store.stored::<usize>(FIRST_SERVER, &slot()).await == Some(expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the refreshed answer was never stored");
}

#[tokio::test]
async fn given_cached_content_when_the_refresh_fails_then_the_content_stays_and_no_error_shows() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store.seed(FIRST_SERVER, &slot(), 7_usize).await;
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let delivered = recorder();

    views.load(
        Some(slot()),
        async { Err::<usize, String>("offline".to_owned()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert_eq!(delivered.lock().unwrap().as_slice(), [Ok(7)]);
    assert_eq!(store.stored::<usize>(FIRST_SERVER, &slot()).await, Some(7));
}

#[tokio::test]
async fn given_no_cache_when_the_fetch_fails_then_the_error_reaches_the_view() {
    let dir = scratch_dir();
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store_in(&dir), &session);
    let delivered = recorder::<usize>();

    views.load(
        Some(slot()),
        async { Err("offline".to_owned()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;

    assert_eq!(
        delivered.lock().unwrap().as_slice(),
        [Err("offline".to_owned())]
    );
}

#[tokio::test]
async fn given_two_servers_when_one_is_cached_then_the_other_sees_nothing() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store.seed(FIRST_SERVER, &slot(), 7_usize).await;
    let session = Session::default();
    let views = views_on(demo_on(SECOND_SERVER), &store, &session);
    let delivered = recorder();

    views.load(Some(slot()), answered(1), recording(&delivered));
    settle_until(|| count_of(&delivered) == 1).await;

    assert_eq!(delivered.lock().unwrap().as_slice(), [Ok(1)]);
}

#[tokio::test]
async fn given_no_slot_when_loaded_then_nothing_is_read_or_written() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let delivered = recorder();

    views.load(None, answered(5), recording(&delivered));
    settle_until(|| count_of(&delivered) == 1).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert_eq!(store.stored::<usize>(FIRST_SERVER, &slot()).await, None);
}

#[tokio::test]
async fn given_a_view_was_left_when_its_fetch_is_pending_then_it_stops_and_delivers_nothing() {
    let dir = scratch_dir();
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store_in(&dir), &session);
    let delivered = recorder::<usize>();
    let (dropped, fetch_dropped) = oneshot::channel::<()>();

    views.load(
        Some(slot()),
        async move {
            let _notifies_when_dropped = dropped;
            pending::<Result<usize, String>>().await
        },
        recording(&delivered),
    );
    session.open_view();

    assert!(fetch_dropped.await.is_err());
    assert_eq!(count_of(&delivered), 0);
}

fn favorite_song() -> Song {
    Song {
        is_favorite: true,
        ..test_song("song-1", 1_000)
    }
}

fn song_view() -> ViewSlot {
    View::Tracks.slot("page-0")
}

async fn first_song_delivered(views: &Views) -> Vec<bool> {
    let delivered = recorder::<Vec<Song>>();
    views.load(
        Some(song_view()),
        async { Err::<Vec<Song>, String>("offline".to_owned()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;
    let results = delivered.lock().unwrap();
    results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .flatten()
        .map(|song| song.is_favorite)
        .collect()
}

#[tokio::test]
async fn given_an_unfavorite_when_a_stale_snapshot_is_shown_then_the_edit_is_not_undone() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let server = demo_on(FIRST_SERVER);
    store
        .seed(
            server.config().id.as_str(),
            &song_view(),
            vec![favorite_song()],
        )
        .await;
    let session = Session::default();
    let views = views_on(server, &store, &session);

    views
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], false)
        .await
        .unwrap();

    assert_eq!(first_song_delivered(&views).await, [false]);
}

#[tokio::test]
async fn given_a_favorite_edit_when_the_app_restarts_then_a_stale_snapshot_still_respects_it() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store
        .seed(FIRST_SERVER, &song_view(), vec![favorite_song()])
        .await;
    let session = Session::default();
    views_on(demo_on(FIRST_SERVER), &store, &session)
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], false)
        .await
        .unwrap();

    let restarted = views_on(demo_on(FIRST_SERVER), &store_in(&dir), &session);

    assert_eq!(first_song_delivered(&restarted).await, [false]);
}

#[tokio::test]
async fn given_a_snapshot_newer_than_the_edit_when_shown_then_the_server_wins() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    views
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], false)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    store
        .seed(FIRST_SERVER, &song_view(), vec![favorite_song()])
        .await;

    assert_eq!(first_song_delivered(&views).await, [true]);
}

#[tokio::test]
async fn given_a_refused_edit_when_a_snapshot_is_shown_then_the_old_state_returns() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let server: Arc<dyn MusicServer> = Arc::new(RefusingServer::new(20));
    let server_id = server.config().id.clone();
    store
        .seed(&server_id, &song_view(), vec![favorite_song()])
        .await;
    let session = Session::default();
    let views = views_on(server, &store, &session);

    let outcome = views
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], false)
        .await;

    assert!(outcome.is_err());
    assert_eq!(first_song_delivered(&views).await, [true]);
}

#[tokio::test]
async fn given_a_refresh_in_flight_when_an_edit_lands_then_the_refresh_keeps_the_edit() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let delivered = recorder::<Vec<Song>>();
    let (answer, answer_received) = oneshot::channel::<Vec<Song>>();
    let (asked, server_asked) = oneshot::channel::<()>();
    views.load(
        Some(song_view()),
        async move {
            asked.send(()).unwrap();
            Ok(answer_received.await.unwrap())
        },
        recording(&delivered),
    );
    server_asked.await.unwrap();

    views
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], false)
        .await
        .unwrap();
    answer.send(vec![favorite_song()]).unwrap();
    settle_until(|| count_of(&delivered) == 1).await;

    let results = delivered.lock().unwrap();
    let songs = results.first().unwrap().as_ref().unwrap();
    assert!(!songs.first().unwrap().is_favorite);
}

#[tokio::test]
async fn given_a_rating_edit_when_a_stale_snapshot_is_shown_then_the_rating_is_kept() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store
        .seed(FIRST_SERVER, &song_view(), vec![test_song("song-1", 1_000)])
        .await;
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);

    views.set_rating(ItemKind::Song, "song-1", 4).await.unwrap();

    let delivered = recorder::<Vec<Song>>();
    views.load(
        Some(song_view()),
        async { Err::<Vec<Song>, String>("offline".to_owned()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;
    let results = delivered.lock().unwrap();
    let songs = results.first().unwrap().as_ref().unwrap();
    assert_eq!(songs.first().unwrap().user_rating, Some(4));
}

#[tokio::test]
async fn given_a_refresh_in_flight_when_its_group_is_invalidated_then_the_answer_is_not_stored() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store.seed(FIRST_SERVER, &slot(), 7_usize).await;
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let delivered = recorder();
    let (answer, answer_received) = oneshot::channel::<usize>();
    views.load(
        Some(slot()),
        async move { Ok(answer_received.await.unwrap()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;

    views.invalidate(View::Albums).await;
    assert_eq!(store.stored::<usize>(FIRST_SERVER, &slot()).await, None);

    answer.send(9).unwrap();
    settle_until(|| count_of(&delivered) == 2).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(store.stored::<usize>(FIRST_SERVER, &slot()).await, None);
}

#[tokio::test]
async fn given_a_favorite_edit_when_it_lands_then_the_favorites_view_is_dropped() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let favorites = View::Favorites.slot("page-0");
    store.seed(FIRST_SERVER, &favorites, 7_usize).await;
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);

    views
        .set_favorite(ItemKind::Song, &["song-1".to_owned()], true)
        .await
        .unwrap();

    assert_eq!(store.stored::<usize>(FIRST_SERVER, &favorites).await, None);
}

#[tokio::test]
async fn given_a_removed_server_when_forgotten_then_its_views_go_and_others_stay() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    store.seed(FIRST_SERVER, &slot(), 7_usize).await;
    store.seed(SECOND_SERVER, &slot(), 8_usize).await;

    store.forget_server(FIRST_SERVER).await;

    assert_eq!(store.stored::<usize>(FIRST_SERVER, &slot()).await, None);
    assert_eq!(store.stored::<usize>(SECOND_SERVER, &slot()).await, Some(8));
}

const COVER_KEY: &str = "cover-key";
const COVER_URL: &str = "https://covers.example/one.jpg";
const COVER_FILE: &str = "/covers/one.jpg";
const COVER_SIZE: u32 = 8;

fn favorite_artist() -> Artist {
    Artist {
        id: "artist-1".to_owned(),
        server_id: FIRST_SERVER.to_owned(),
        name: "Artist".to_owned(),
        album_count: None,
        song_count: None,
        cover_art_id: None,
        is_favorite: true,
        user_rating: None,
        biography: None,
    }
}

fn favorite_album() -> Album {
    Album {
        id: "album-1".to_owned(),
        server_id: FIRST_SERVER.to_owned(),
        name: "Album".to_owned(),
        album_artists: Vec::new(),
        artists: Vec::new(),
        year: None,
        genres: Vec::new(),
        song_count: 1,
        duration_ms: 1_000,
        cover_art_id: None,
        play_count: 0,
        user_rating: None,
        is_favorite: true,
        created_at: None,
        last_played_at: None,
    }
}

async fn clear_favorites(views: &Views) {
    for (kind, id) in [
        (ItemKind::Artist, "artist-1"),
        (ItemKind::Album, "album-1"),
        (ItemKind::Song, "song-1"),
    ] {
        views
            .set_favorite(kind, &[id.to_owned()], false)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn given_unfavorites_when_a_stale_artist_page_is_shown_then_none_is_undone() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let server = demo_on(FIRST_SERVER);
    let artist_view = View::Artist("artist-1".to_owned()).slot("detail");
    let stale = ArtistDetail {
        artist: favorite_artist(),
        albums: vec![favorite_album()],
        top_songs: vec![favorite_song()],
    };
    store
        .seed(server.config().id.as_str(), &artist_view, stale)
        .await;
    let session = Session::default();
    let views = views_on(server, &store, &session);
    clear_favorites(&views).await;

    let delivered = recorder::<ArtistDetail>();
    views.load(
        Some(artist_view),
        async { Err::<ArtistDetail, String>("offline".to_owned()) },
        recording(&delivered),
    );
    settle_until(|| count_of(&delivered) == 1).await;

    let results = delivered.lock().unwrap();
    let shown = results.first().unwrap().as_ref().unwrap();
    assert!(!shown.artist.is_favorite);
    assert!(!shown.albums.first().unwrap().is_favorite);
    assert!(!shown.top_songs.first().unwrap().is_favorite);
}

#[tokio::test]
async fn given_unfavorites_when_stale_artist_parts_arrive_then_none_is_undone() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let taken_at_ms = Views::taken_now();
    clear_favorites(&views).await;

    let mut profile = ArtistPart::Profile(favorite_artist());
    let mut albums = ArtistPart::Albums(vec![favorite_album()]);
    let mut top_songs = ArtistPart::TopSongs(vec![favorite_song()]);
    let mut biography = ArtistPart::Biography(Some("Biography".to_owned()));
    views.overlay(taken_at_ms, &mut profile);
    views.overlay(taken_at_ms, &mut albums);
    views.overlay(taken_at_ms, &mut top_songs);
    views.overlay(taken_at_ms, &mut biography);

    let unfavorited_artist = Artist {
        is_favorite: false,
        ..favorite_artist()
    };
    let unfavorited_album = Album {
        is_favorite: false,
        ..favorite_album()
    };
    let unfavorited_song = Song {
        is_favorite: false,
        ..favorite_song()
    };
    assert_eq!(profile, ArtistPart::Profile(unfavorited_artist));
    assert_eq!(albums, ArtistPart::Albums(vec![unfavorited_album]));
    assert_eq!(top_songs, ArtistPart::TopSongs(vec![unfavorited_song]));
    assert_eq!(
        biography,
        ArtistPart::Biography(Some("Biography".to_owned()))
    );
}

#[tokio::test]
async fn given_unfavorites_when_a_stale_pair_is_shown_then_both_sides_keep_them() {
    let dir = scratch_dir();
    let store = store_in(&dir);
    let session = Session::default();
    let views = views_on(demo_on(FIRST_SERVER), &store, &session);
    let taken_at_ms = Views::taken_now();
    clear_favorites(&views).await;

    let mut stale = (vec![favorite_song()], Some(favorite_album()));
    views.overlay(taken_at_ms, &mut stale);

    let (songs, album) = stale;
    assert!(!songs.first().unwrap().is_favorite);
    assert!(!album.unwrap().is_favorite);
}

struct HangingCovers;

#[async_trait]
impl CoverProvider for HangingCovers {
    async fn cover(
        &self,
        _server: &dyn MusicServer,
        _cover_art_id: &str,
        _size: u32,
    ) -> anyhow::Result<Arc<DecodedImage>> {
        pending().await
    }

    async fn cover_from_url(
        &self,
        _cache_key: &str,
        _url: &str,
        _size: u32,
    ) -> anyhow::Result<Arc<DecodedImage>> {
        pending().await
    }

    fn cover_file(
        &self,
        _server: &dyn MusicServer,
        _cover_art_id: &str,
        _size: u32,
    ) -> Option<PathBuf> {
        Some(PathBuf::from(COVER_FILE))
    }
}

#[tokio::test]
async fn given_a_view_was_left_when_its_url_cover_is_pending_then_it_stops_waiting() {
    let session = Session::default();
    let covers = FocusedCovers::new(Arc::new(HangingCovers), session.clone());
    let leave_view = async {
        tokio::task::yield_now().await;
        session.open_view();
    };

    let (cover, ()) = tokio::join!(
        covers.cover_from_url(COVER_KEY, COVER_URL, COVER_SIZE),
        leave_view
    );

    assert!(cover.is_err());
}

#[tokio::test]
async fn given_an_open_view_when_a_url_cover_is_asked_then_it_arrives() {
    let covers = FocusedCovers::new(Arc::new(DemoCovers), Session::default());

    let cover = covers
        .cover_from_url(COVER_KEY, COVER_URL, COVER_SIZE)
        .await
        .unwrap();

    assert_eq!(cover.width, COVER_SIZE);
}

#[test]
fn given_a_cover_file_when_asked_then_it_comes_from_the_wrapped_provider() {
    let covers = FocusedCovers::new(Arc::new(HangingCovers), Session::default());
    let server = demo_on(FIRST_SERVER);

    let file = covers.cover_file(server.as_ref(), "cover-1", COVER_SIZE);

    assert_eq!(file, Some(PathBuf::from(COVER_FILE)));
}
