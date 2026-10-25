use std::cell::RefCell;

use cadence_core::model::{Page, Paging, Song, SongListQuery};
use slint::{Model, ModelTracker};

mod selecting;

use super::paged_songs::{PagedSongs, SongPages};
use super::row::{placeholder_track, track_row};
use crate::ui::TrackRow;
use crate::ui::page_cache::{PageShape, changed_total, page_paging};

pub const PAGE_SIZE: usize = 100;
pub const CACHED_PAGES: usize = 24;
const LOADED_SELECTION_LIMIT: usize = PAGE_SIZE * CACHED_PAGES;
const TRACK_PAGES: PageShape = PageShape {
    page_size: PAGE_SIZE,
    capacity: CACHED_PAGES,
};

pub enum Request {
    Count {
        generation: u64,
        query: SongListQuery,
    },
    Page {
        generation: u64,
        query: SongListQuery,
        page: usize,
    },
}

pub(super) fn paging_for_page(page: usize) -> Paging {
    page_paging(page, PAGE_SIZE)
}

#[derive(Default)]
struct ListState {
    generation: u64,
    query: SongListQuery,
    current_song: Option<String>,
}

fn visible_row(songs: &SongPages, row: usize, current_song: Option<&str>) -> TrackRow {
    let is_selected = songs.is_selected(row);
    let Some(song) = songs.song(row) else {
        return placeholder_row(row, is_selected);
    };
    TrackRow {
        is_playing: current_song == Some(song.id.as_str()),
        is_selected,
        ..track_row(song, row)
    }
}

fn placeholder_row(row: usize, is_selected: bool) -> TrackRow {
    TrackRow {
        is_selected,
        ..placeholder_track(row)
    }
}

pub struct TracksModel {
    state: RefCell<ListState>,
    songs: PagedSongs,
    fetch: Box<dyn Fn(Request)>,
}

impl TracksModel {
    pub fn new(fetch: impl Fn(Request) + 'static) -> Self {
        Self {
            state: RefCell::default(),
            songs: PagedSongs::new(TRACK_PAGES),
            fetch: Box::new(fetch),
        }
    }

    pub fn query(&self) -> SongListQuery {
        self.state.borrow().query.clone()
    }

    pub fn reset(&self, query: SongListQuery) {
        let generation = {
            let mut state = self.state.borrow_mut();
            state.generation += 1;
            state.query = query.clone();
            state.generation
        };
        self.songs.reset();
        (self.fetch)(Request::Count { generation, query });
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.state.borrow().generation == generation
    }

    pub fn reported_total_change(
        &self,
        generation: u64,
        reported_total: Option<usize>,
    ) -> Option<usize> {
        if !self.is_current(generation) {
            return None;
        }
        changed_total(self.songs.total(), reported_total)
    }

    pub fn apply_count(&self, generation: u64, total: usize) {
        if self.is_current(generation) {
            self.songs.set_total(total);
        }
    }

    pub fn apply_page(&self, generation: u64, page: usize, songs: Vec<Song>) {
        if self.is_current(generation) {
            self.songs.apply_page(page, songs);
        }
    }

    pub fn apply_listed_page(&self, generation: u64, page: usize, listed: Page<Song>) {
        match self.reported_total_change(generation, listed.total_count) {
            Some(total) => self.relay_rows(page, total, listed.items),
            None => self.apply_page(generation, page, listed.items),
        }
    }

    fn relay_rows(&self, page: usize, total: usize, songs: Vec<Song>) {
        tracing::debug!(
            total,
            "the server list changed size, reloading the cached pages"
        );
        self.state.borrow_mut().generation += 1;
        self.songs.relay(page, total, songs);
    }

    pub fn page_failed(&self, generation: u64, page: usize) {
        if self.is_current(generation) {
            self.songs.page_failed(page);
        }
    }

    pub fn song_at(&self, index: usize) -> Option<Song> {
        self.songs.song(index)
    }

    pub fn song_id_at(&self, index: usize) -> Option<String> {
        self.songs.song_id(index)
    }

    pub fn set_current(&self, song_id: Option<&str>) {
        let previous = {
            let mut state = self.state.borrow_mut();
            if state.current_song.as_deref() == song_id {
                return;
            }
            std::mem::replace(&mut state.current_song, song_id.map(str::to_owned))
        };
        let changed = {
            let songs = self.songs.read();
            [previous.as_deref(), song_id].map(|id| id.and_then(|id| songs.index_of(id)))
        };
        self.songs.notify_rows(changed.into_iter().flatten());
    }

    pub fn update_songs(&self, ids: &[String], update: impl Fn(&mut Song)) {
        self.songs.update_songs(ids, update);
    }

    fn request_pages_around(&self, row: usize) {
        self.songs.focus_on(row);
        let pages = self.songs.claim_pages_around(row);
        let (generation, query) = {
            let state = self.state.borrow();
            (state.generation, state.query.clone())
        };
        for page in pages {
            (self.fetch)(Request::Page {
                generation,
                query: query.clone(),
                page,
            });
        }
    }
}

impl Model for TracksModel {
    type Data = TrackRow;

    fn row_count(&self) -> usize {
        self.songs.total()
    }

    fn row_data(&self, row: usize) -> Option<TrackRow> {
        if row >= self.songs.total() {
            return None;
        }
        self.request_pages_around(row);
        let current_song = self.state.borrow().current_song.clone();
        Some(visible_row(
            &self.songs.read(),
            row,
            current_song.as_deref(),
        ))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        self.songs.tracker()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
