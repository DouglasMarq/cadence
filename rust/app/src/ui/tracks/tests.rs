mod columns;
mod eviction;
mod headless;
mod listed;
mod ranges;
mod session;
mod widths;

use std::cell::RefCell;
use std::rc::Rc;

use cadence_api::MusicServer;
use cadence_core::model::{Paging, Song, SongListQuery, SongSort};
use slint::Model;

use super::listed::{LISTED_CHUNK_SIZE, ListedSongs};
use super::model::{PAGE_SIZE, Request, TracksModel};
use super::query::{Filters, parse_year};
use super::row::track_row;
use super::selection::{SelectedSongs, Selection};
use crate::demo::{FakeServer, test_song};
use crate::list_stream::RowRanges;
use crate::ui::TrackColumn;

type RequestLog = Rc<RefCell<Vec<(u64, Option<usize>)>>>;

fn drain(log: &RequestLog) -> Vec<(u64, Option<usize>)> {
    log.borrow_mut().drain(..).collect()
}

fn logging_model() -> (TracksModel, RequestLog) {
    let log: RequestLog = Rc::default();
    let sink = Rc::clone(&log);
    let model = TracksModel::new(move |request| {
        sink.borrow_mut().push(match request {
            Request::Count { generation, .. } => (generation, None),
            Request::Page {
                generation, page, ..
            } => (generation, Some(page)),
        });
    });
    (model, log)
}

fn page_of(page: usize, len: usize) -> Vec<Song> {
    (0..len)
        .map(|position| test_song(&format!("s{}", page * PAGE_SIZE + position), 1000))
        .collect()
}

#[test]
fn pages_are_requested_once_and_stale_generations_ignored() {
    let (model, log) = logging_model();
    model.reset(SongListQuery::default());
    assert_eq!(drain(&log), [(1, None)]);
    model.apply_count(1, 250);
    assert_eq!(model.row_count(), 250);

    assert!(model.row_data(150).is_some_and(|row| !row.is_loaded));
    model.row_data(160);
    assert_eq!(drain(&log), [(1, Some(1)), (1, Some(2))]);

    model.apply_page(1, 1, vec![test_song("x", 1000)]);
    model.set_current(Some("x"));
    assert!(
        model
            .row_data(100)
            .is_some_and(|row| row.is_loaded && row.is_playing)
    );
    assert_eq!(model.song_id_at(100).as_deref(), Some("x"));

    model.reset(SongListQuery::default());
    assert!(!model.is_current(1));
    model.apply_count(1, 10);
    assert_eq!(model.row_count(), 0);
}

fn rows_of(ranges: &RowRanges) -> Vec<usize> {
    ranges.rows().collect()
}

fn changed_rows(selection: &mut Selection, change: impl FnOnce(&mut Selection)) -> Vec<usize> {
    let before = selection.ranges().clone();
    change(selection);
    rows_of(&before.symmetric_difference(selection.ranges()))
}

fn selected_after(model: &TracksModel, change: impl FnOnce(&TracksModel)) -> usize {
    change(model);
    model.selected_count()
}

#[test]
fn clicks_follow_desktop_selection_rules() {
    let mut selection = Selection::default();
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(3, false, false)),
        [3]
    );
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(5, false, true)),
        [4, 5]
    );
    assert_eq!(rows_of(selection.ranges()), [3, 4, 5]);

    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(1, false, true)),
        [1, 2, 4, 5]
    );
    assert_eq!(rows_of(selection.ranges()), [1, 2, 3]);

    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(8, true, false)),
        [8]
    );
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(2, true, false)),
        [2]
    );
    assert_eq!(rows_of(selection.ranges()), [1, 3, 8]);

    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(4, true, true)),
        [2, 4]
    );
    assert_eq!(rows_of(selection.ranges()), [1, 2, 3, 4, 8]);

    assert!(changed_rows(&mut selection, |selection| selection.focus(3)).is_empty());
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.focus(9)),
        [1, 2, 3, 4, 8, 9]
    );
    assert_eq!(selection.len(), 1);

    let mut fresh = Selection::default();
    assert_eq!(
        changed_rows(&mut fresh, |selection| selection.click(6, false, true)),
        [6]
    );
}

#[test]
fn given_no_anchor_when_a_toggling_range_click_lands_then_it_toggles_that_row() {
    let mut selection = Selection::default();
    selection.click(4, true, true);
    assert_eq!(selection.indexes(), [4]);
    selection.click(4, true, false);
    assert!(selection.indexes().is_empty());
}

#[test]
fn given_a_selection_when_targets_are_asked_for_then_they_follow_the_clicked_row() {
    let mut selection = Selection::default();
    selection.click(1, false, false);
    selection.click(3, true, false);
    assert_eq!(selection.targets(3), [1, 3]);
    assert_eq!(selection.targets(2), [2]);
}

#[test]
fn select_all_and_clear_span_every_row_without_moving_the_anchor() {
    let mut selection = Selection::default();
    selection.click(2, true, false);
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.select_all(5)),
        [0, 1, 3, 4]
    );
    assert_eq!(selection.len(), 5);
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(4, false, true)),
        [0, 1]
    );
    assert_eq!(rows_of(selection.ranges()), [2, 3, 4]);
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.clear()),
        [2, 3, 4]
    );
    assert_eq!(selection.len(), 0);
    assert_eq!(
        changed_rows(&mut selection, |selection| selection.click(1, false, true)),
        [1]
    );

    let mut library = Selection::default();
    let before = library.ranges().clone();
    library.select_all(100_000);
    assert_eq!(
        before.symmetric_difference(library.ranges()),
        RowRanges::spanning(0..100_000)
    );
    assert_eq!(library.ranges(), &RowRanges::spanning(0..100_000));
    assert_eq!(library.len(), 100_000);
    let before = library.ranges().clone();
    library.select_all(0);
    assert_eq!(
        before.symmetric_difference(library.ranges()).row_count(),
        100_000
    );
}

fn model_with_first_page_of(total: usize) -> TracksModel {
    let (model, _log) = logging_model();
    model.reset(SongListQuery::default());
    model.apply_count(1, total);
    model.apply_page(1, 0, page_of(0, PAGE_SIZE));
    model
}

#[test]
fn selection_marks_rows_and_resets_with_the_query() {
    let model = model_with_first_page_of(250);

    assert_eq!(
        selected_after(&model, |model| model.select(2, false, false)),
        1
    );
    assert_eq!(
        selected_after(&model, |model| model.select(150, false, true)),
        149
    );
    assert!(model.row_data(2).is_some_and(|row| row.is_selected));
    assert!(
        model
            .row_data(120)
            .is_some_and(|row| row.is_selected && !row.is_loaded)
    );
    assert!(model.row_data(1).is_some_and(|row| !row.is_selected));
    assert_eq!(
        selected_after(&model, |model| model.select(400, false, false)),
        149
    );

    let SelectedSongs::Listed(listed) = model.selected_songs() else {
        panic!("a selection over unloaded rows is listed");
    };
    assert_eq!(listed.ranges, RowRanges::spanning(2..151));
    let first = model.first_selected();
    assert_eq!(
        first.and_then(|first| first.song).map(|song| song.id),
        Some("s2".into())
    );

    assert_eq!(
        selected_after(&model, |model| model.select(5, false, true)),
        4
    );
    let SelectedSongs::Loaded(songs) = model.selected_songs() else {
        panic!("a selection over cached rows is loaded");
    };
    let ids: Vec<String> = songs.into_iter().map(|song| song.id).collect();
    assert_eq!(ids, ["s2", "s3", "s4", "s5"]);
}

#[test]
fn given_a_selection_when_everything_is_selected_cleared_or_reset_then_rows_follow() {
    let model = model_with_first_page_of(250);

    assert_eq!(selected_after(&model, |model| model.select_all()), 250);
    assert!(model.row_data(249).is_some_and(|row| row.is_selected));
    assert_eq!(selected_after(&model, |model| model.clear_selection()), 0);
    assert!(model.row_data(2).is_some_and(|row| !row.is_selected));

    model.select(1, false, false);
    model.reset(SongListQuery::default());
    assert!(matches!(model.selected_songs(), SelectedSongs::Loaded(songs) if songs.is_empty()));
    assert!(model.first_selected().is_none());
}

#[test]
fn updates_reach_loaded_rows() {
    let (model, _log) = logging_model();
    model.reset(SongListQuery::default());
    model.apply_count(1, 10);
    model.apply_page(1, 0, page_of(0, 10));
    model.update_songs(&["s4".into()], |song| {
        song.is_favorite = true;
        song.user_rating = Some(4);
    });
    let row = model.row_data(4).unwrap_or_default();
    assert!(row.is_favorite);
    assert_eq!(row.rating, 4);
    assert!(model.row_data(3).is_some_and(|row| !row.is_favorite));
}

async fn songs_in(server: &FakeServer, query: &SongListQuery, paging: Paging) -> Vec<Song> {
    server
        .song_list(query, paging)
        .await
        .map(|page| page.items)
        .unwrap_or_default()
}

async fn listed_songs(server: &FakeServer, listed: ListedSongs) -> Vec<Song> {
    let mut chunks = listed.into_chunks(LISTED_CHUNK_SIZE);
    let mut songs = Vec::new();
    while let Ok(Some(chunk)) = chunks.next_songs(server).await {
        songs.extend(chunk);
    }
    songs
}

#[test]
fn filters_feed_the_query() {
    let base = SongListQuery {
        sort: SongSort::Year,
        search: Some("river".into()),
        ..SongListQuery::default()
    };
    let query = Filters {
        is_favorite_only: true,
        genre_id: Some("genre-2".into()),
        year_from: Some(2000),
        year_to: Some(1990),
    }
    .apply(base.clone());
    assert_eq!(query.is_favorite, Some(true));
    assert_eq!(query.genre_ids, ["genre-2"]);
    assert_eq!((query.year_from, query.year_to), (Some(1990), Some(2000)));
    assert_eq!(
        (query.sort, query.search.as_deref()),
        (SongSort::Year, Some("river"))
    );

    let cleared = Filters::default().apply(query);
    assert_eq!(cleared, base);

    assert_eq!(parse_year(" 1999 "), Some(1999));
    assert_eq!(parse_year(""), None);
    assert_eq!(parse_year("0"), None);
    assert_eq!(parse_year("abc"), None);
}

#[test]
fn rows_carry_links_favorite_and_rating() {
    let server = FakeServer::new(24);
    let song = Song {
        is_favorite: true,
        user_rating: Some(3),
        ..server.song_at(13)
    };
    let row = track_row(&song, 13);
    assert_eq!(row.index, 14);
    assert_eq!(row.album_id, "album-1");
    assert!(row.artist_id.starts_with("artist-"));
    assert!(row.is_favorite);
    assert_eq!(row.rating, 3);
    assert_eq!(TrackColumn::Favorite.sort(), SongSort::Favorited);
    assert_eq!(TrackColumn::Rating.sort(), SongSort::Rating);
}

#[test]
fn given_each_track_column_its_settings_id_maps_back_to_the_same_column() {
    for column in TrackColumn::ALL {
        assert_eq!(TrackColumn::from_id(column.id()), Some(column));
    }
    assert_eq!(TrackColumn::Plays.id(), "plays");
}

#[test]
fn given_an_unknown_settings_column_id_it_maps_to_no_column() {
    assert_eq!(TrackColumn::from_id("bitrate"), None);
}
