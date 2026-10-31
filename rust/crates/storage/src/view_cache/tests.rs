use std::path::Path;

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Shelf {
    titles: Vec<String>,
}

fn shelf(title: &str) -> CachedView<Shelf> {
    CachedView {
        taken_at_ms: 1_000,
        payload: Shelf {
            titles: vec![title.to_owned()],
        },
    }
}

fn key(server_id: &str, item: &str) -> ViewKey {
    ViewKey::new(server_id, "albums", item)
}

fn cache_in(directory: &Path) -> ViewCache {
    ViewCache::bounded(directory.join(VIEW_CACHE_DIRECTORY), 1 << 20)
}

fn entry_files(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory.join(VIEW_CACHE_DIRECTORY))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}

#[tokio::test]
async fn given_a_saved_view_when_the_app_restarts_then_it_is_read_back() {
    let directory = tempfile::tempdir().unwrap();
    cache_in(directory.path())
        .write(&key("server-1", "page-0"), shelf("Blue"))
        .await;
    let restarted = cache_in(directory.path());
    let view = restarted.read::<Shelf>(&key("server-1", "page-0")).await;
    assert_eq!(view, Some(shelf("Blue")));
}

#[tokio::test]
async fn given_a_view_of_one_server_when_another_reads_the_same_view_then_it_finds_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    cache.write(&key("server-1", "page-0"), shelf("Blue")).await;
    assert_eq!(cache.read::<Shelf>(&key("server-2", "page-0")).await, None);
}

#[tokio::test]
async fn given_two_servers_with_the_same_view_when_each_saves_then_they_stay_apart() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    cache.write(&key("server-1", "page-0"), shelf("Blue")).await;
    cache.write(&key("server-2", "page-0"), shelf("Red")).await;
    let first = cache.read::<Shelf>(&key("server-1", "page-0")).await;
    let second = cache.read::<Shelf>(&key("server-2", "page-0")).await;
    assert_eq!(first, Some(shelf("Blue")));
    assert_eq!(second, Some(shelf("Red")));
}

#[tokio::test]
async fn given_a_corrupt_entry_when_reading_then_it_is_ignored_and_removed() {
    let directory = tempfile::tempdir().unwrap();
    let entry_key = key("server-1", "page-0");
    cache_in(directory.path())
        .write(&entry_key, shelf("Blue"))
        .await;
    let file = entry_files(directory.path()).remove(0);
    std::fs::write(&file, b"{not json").unwrap();
    let restarted = cache_in(directory.path());
    assert_eq!(restarted.read::<Shelf>(&entry_key).await, None);
    assert!(!file.exists());
}

#[tokio::test]
async fn given_an_entry_of_an_old_format_when_reading_then_it_is_ignored_and_removed() {
    let directory = tempfile::tempdir().unwrap();
    let entry_key = key("server-1", "page-0");
    cache_in(directory.path())
        .write(&entry_key, shelf("Blue"))
        .await;
    let file = entry_files(directory.path()).remove(0);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replacen("\"format\":1", "\"format\":0", 1)).unwrap();
    let restarted = cache_in(directory.path());
    assert_eq!(restarted.read::<Shelf>(&entry_key).await, None);
    assert!(!file.exists());
}

#[tokio::test]
async fn given_an_entry_whose_payload_no_longer_decodes_when_reading_then_it_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let entry_key = key("server-1", "page-0");
    let cache = cache_in(directory.path());
    cache.write(&entry_key, shelf("Blue")).await;
    assert_eq!(cache.read::<Vec<u32>>(&entry_key).await, None);
    assert!(entry_files(directory.path()).is_empty());
}

#[tokio::test]
async fn given_a_file_copied_from_another_view_when_reading_then_it_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    cache.write(&key("server-1", "page-0"), shelf("Blue")).await;
    cache.write(&key("server-2", "page-0"), shelf("Red")).await;
    let files = entry_files(directory.path());
    std::fs::copy(&files[0], &files[1]).unwrap();
    let readable = [
        cache.read::<Shelf>(&key("server-1", "page-0")).await,
        cache.read::<Shelf>(&key("server-2", "page-0")).await,
    ];
    assert_eq!(readable.iter().flatten().count(), 1);
}

#[tokio::test]
async fn given_a_disk_bound_when_more_views_are_saved_then_the_oldest_are_evicted() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ViewCache::bounded(directory.path().join(VIEW_CACHE_DIRECTORY), 400);
    for page in 0..20 {
        cache
            .write(&key("server-1", &format!("page-{page}")), shelf("Blue"))
            .await;
    }
    assert!(cache.disk_usage().await <= 400);
    assert_eq!(cache.read::<Shelf>(&key("server-1", "page-0")).await, None);
    assert!(
        cache
            .read::<Shelf>(&key("server-1", "page-19"))
            .await
            .is_some()
    );
}

#[tokio::test]
async fn given_a_recently_read_view_when_the_bound_is_reached_then_it_survives() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ViewCache::bounded(directory.path().join(VIEW_CACHE_DIRECTORY), 400);
    cache.write(&key("server-1", "kept"), shelf("Blue")).await;
    for page in 0..20 {
        assert!(
            cache
                .read::<Shelf>(&key("server-1", "kept"))
                .await
                .is_some()
        );
        cache
            .write(&key("server-1", &format!("page-{page}")), shelf("Blue"))
            .await;
    }
    assert!(
        cache
            .read::<Shelf>(&key("server-1", "kept"))
            .await
            .is_some()
    );
}

#[tokio::test]
async fn given_a_bound_lowered_since_the_last_run_when_the_app_restarts_then_it_is_enforced() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    for page in 0..20 {
        cache
            .write(&key("server-1", &format!("page-{page}")), shelf("Blue"))
            .await;
    }
    let restarted = ViewCache::bounded(directory.path().join(VIEW_CACHE_DIRECTORY), 400);
    assert!(restarted.disk_usage().await <= 400);
}

#[tokio::test]
async fn given_a_view_larger_than_an_entry_may_be_when_saving_then_it_is_not_stored() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    let entry_key = key("server-1", "huge");
    cache.write(&entry_key, shelf("Blue")).await;
    let huge = CachedView {
        taken_at_ms: 2_000,
        payload: vec![0_u8; MAX_ENTRY_BYTES],
    };
    cache.write(&entry_key, huge).await;
    assert_eq!(cache.read::<Shelf>(&entry_key).await, None);
    assert_eq!(cache.disk_usage().await, 0);
}

#[tokio::test]
async fn given_a_group_when_it_is_forgotten_then_only_its_entries_go() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    let album = ViewKey::new("server-1", "album-7", "detail");
    let playlist = ViewKey::new("server-1", "playlist-7", "detail");
    let other_server = ViewKey::new("server-2", "album-7", "detail");
    for entry_key in [&album, &playlist, &other_server] {
        cache.write(entry_key, shelf("Blue")).await;
    }
    cache.forget_group(&album).await;
    assert_eq!(cache.read::<Shelf>(&album).await, None);
    assert!(cache.read::<Shelf>(&playlist).await.is_some());
    assert!(cache.read::<Shelf>(&other_server).await.is_some());
}

#[tokio::test]
async fn given_a_removed_server_when_it_is_forgotten_then_only_its_entries_go() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache_in(directory.path());
    cache.write(&key("server-1", "page-0"), shelf("Blue")).await;
    cache.write(&key("server-2", "page-0"), shelf("Red")).await;
    cache.forget_server("server-1").await;
    assert_eq!(cache.read::<Shelf>(&key("server-1", "page-0")).await, None);
    assert!(
        cache
            .read::<Shelf>(&key("server-2", "page-0"))
            .await
            .is_some()
    );
}

#[tokio::test]
async fn given_a_cache_on_a_read_only_location_when_used_then_it_only_finds_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, b"a file where the folder should be").unwrap();
    let cache = ViewCache::bounded(blocked.join(VIEW_CACHE_DIRECTORY), 1 << 20);
    cache.write(&key("server-1", "page-0"), shelf("Blue")).await;
    assert_eq!(cache.read::<Shelf>(&key("server-1", "page-0")).await, None);
}
