use std::cell::RefCell;
use std::rc::Rc;

use cadence_core::model::{Folder, FolderId, Song};
use slint::Model;

use super::model::{FirstSongs, FolderRowsModel, SONG_PAGE_SIZE, SongPageRequest};
use crate::demo::test_song;

type Requests = Rc<RefCell<Vec<SongPageRequest>>>;

fn recording_model() -> (FolderRowsModel, Requests) {
    let requests = Requests::default();
    let sink = Rc::clone(&requests);
    let model = FolderRowsModel::new(move |request| sink.borrow_mut().push(request));
    (model, requests)
}

fn folders(count: usize) -> Vec<Folder> {
    (0..count)
        .map(|index| Folder {
            id: format!("dir-{index}"),
            name: format!("Folder {index}"),
            cover_art_id: None,
        })
        .collect()
}

fn contents(
    folders: Vec<Folder>,
    song_total: usize,
    songs: Vec<Song>,
) -> (Vec<Folder>, FirstSongs) {
    (folders, FirstSongs { song_total, songs })
}

fn show(model: &FolderRowsModel, generation: u64, contents: (Vec<Folder>, FirstSongs)) {
    let (shown_folders, first_songs) = contents;
    model.show_folders(generation, shown_folders);
    model.show_first_songs(generation, first_songs);
}

fn songs_of_page(page: usize) -> Vec<Song> {
    (0..SONG_PAGE_SIZE)
        .map(|offset| test_song(&format!("s{}", page * SONG_PAGE_SIZE + offset), 1000))
        .collect()
}

fn taken_pages(requests: &Requests) -> Vec<usize> {
    requests
        .borrow_mut()
        .drain(..)
        .map(|request| request.page)
        .collect()
}

#[test]
fn folders_come_first_and_songs_page_in_on_demand() {
    let (model, requests) = recording_model();
    let generation = model.reset(FolderId::directory("artist"));
    show(
        &model,
        generation,
        contents(folders(3), 350, songs_of_page(0)),
    );
    assert_eq!(model.row_count(), 353);

    let folder = model.row_data(0).unwrap();
    assert!(folder.is_folder && folder.folder_id == "dir-0");
    let song = model.row_data(4).unwrap();
    assert!(!song.is_folder && song.track.is_loaded && song.track.id == "s1");
    assert_eq!(model.song_at(4).map(|(position, _)| position), Some(1));
    assert!(model.song_at(1).is_none());

    let placeholder = model.row_data(3 + 150).unwrap();
    assert!(!placeholder.track.is_loaded);
    assert_eq!(placeholder.track.index, 151);
    assert_eq!(taken_pages(&requests), [1, 2]);
    assert!(model.row_data(353).is_none());
}

#[test]
fn pages_from_an_older_folder_are_ignored_and_failures_back_off() {
    let (model, requests) = recording_model();
    let old = model.reset(FolderId::directory("old"));
    let current = model.reset(FolderId::directory("new"));
    assert!(!model.is_current(old));
    show(&model, old, contents(folders(1), 10, Vec::new()));
    show(&model, current, contents(Vec::new(), 250, songs_of_page(0)));

    model.row_data(220);
    let pending = requests.borrow_mut().drain(..).collect::<Vec<_>>();
    assert!(
        pending
            .iter()
            .all(|request| request.folder == FolderId::directory("new"))
    );
    let stale = SongPageRequest {
        generation: old,
        ..pending[0].clone()
    };
    model.apply_song_page(&stale, songs_of_page(2));
    assert!(model.song_at(220).is_none());

    assert!(model.is_current(pending[0].generation));
    model.song_page_failed(&pending[0]);
    model.row_data(220);
    assert!(taken_pages(&requests).is_empty());
    model.apply_song_page(&pending[0], songs_of_page(2));
    assert_eq!(
        model.song_at(220).map(|(_, song)| song.id),
        Some("s220".into())
    );
}

#[test]
fn scrolling_a_huge_folder_keeps_a_bounded_cache() {
    let (model, requests) = recording_model();
    let generation = model.reset(FolderId::directory("flat"));
    show(
        &model,
        generation,
        contents(Vec::new(), 20_000, songs_of_page(0)),
    );
    for row in (0..20_000).step_by(50) {
        model.row_data(row);
        let pending: Vec<SongPageRequest> = requests.borrow_mut().drain(..).collect();
        for request in pending {
            model.apply_song_page(&request, songs_of_page(request.page));
        }
    }
    let cached = (0..20_000)
        .filter(|row| model.song_at(*row).is_some())
        .count();
    assert!(cached <= 8 * SONG_PAGE_SIZE, "{cached} songs cached");
    assert!(model.song_at(19_999).is_some());
}

#[test]
fn given_songs_before_the_subfolders_when_the_subfolders_arrive_then_they_come_first() {
    let (model, _requests) = recording_model();
    let generation = model.reset(FolderId::directory("artist"));
    model.show_first_songs(
        generation,
        FirstSongs {
            song_total: 100,
            songs: songs_of_page(0),
        },
    );
    assert!(!model.is_settled());
    assert_eq!(model.row_count(), 100);

    model.show_folders(generation, folders(2));

    assert!(model.is_settled());
    assert_eq!(model.counts(), (2, 100));
    assert!(model.row_data(0).unwrap().is_folder);
    assert!(!model.row_data(2).unwrap().is_folder);
}

#[test]
fn given_a_shown_folder_when_a_refresh_changes_the_song_total_then_later_pages_are_asked_again() {
    let (model, requests) = recording_model();
    let generation = model.reset(FolderId::directory("artist"));
    show(
        &model,
        generation,
        contents(Vec::new(), 250, songs_of_page(0)),
    );
    model.row_data(150);
    taken_pages(&requests);

    show(
        &model,
        generation,
        contents(Vec::new(), 240, songs_of_page(0)),
    );
    model.row_data(150);

    assert!(taken_pages(&requests).contains(&1));
}
