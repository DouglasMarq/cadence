use std::cell::{Ref, RefCell};
use std::collections::HashSet;
use std::ops::Range;

use cadence_core::model::Song;
use slint::{ModelNotify, ModelTracker};

use super::selection::Selection;
use crate::list_stream::RowRanges;
use crate::ui::page_cache::{PageCache, PageShape};

pub struct SongPages {
    total: usize,
    pages: PageCache<Song>,
    selection: Selection,
}

impl SongPages {
    fn new(shape: PageShape) -> Self {
        Self {
            total: 0,
            pages: PageCache::new(shape),
            selection: Selection::default(),
        }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn song(&self, index: usize) -> Option<&Song> {
        self.pages.get(index)
    }

    pub fn index_of(&self, song_id: &str) -> Option<usize> {
        self.pages
            .rows()
            .find_map(|(index, song)| (song.id == song_id).then_some(index))
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selection.contains(index)
    }

    fn rows_of_page(&self, page: usize, song_count: usize) -> Range<usize> {
        let start = self.pages.first_index_of(page);
        start..start.saturating_add(song_count).min(self.total)
    }
}

pub struct PagedSongs {
    shape: PageShape,
    state: RefCell<SongPages>,
    notify: ModelNotify,
}

impl PagedSongs {
    pub fn new(shape: PageShape) -> Self {
        Self {
            shape,
            state: RefCell::new(SongPages::new(shape)),
            notify: ModelNotify::default(),
        }
    }

    pub fn tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    pub fn read(&self) -> Ref<'_, SongPages> {
        self.state.borrow()
    }

    pub fn total(&self) -> usize {
        self.state.borrow().total
    }

    pub fn song(&self, index: usize) -> Option<Song> {
        self.state.borrow().song(index).cloned()
    }

    pub fn song_id(&self, index: usize) -> Option<String> {
        self.state.borrow().song(index).map(|song| song.id.clone())
    }

    pub fn selected_count(&self) -> usize {
        self.state.borrow().selection.len()
    }

    pub fn reset(&self) {
        *self.state.borrow_mut() = SongPages::new(self.shape);
        self.notify.reset();
    }

    pub fn set_total(&self, total: usize) {
        self.state.borrow_mut().total = total;
        self.notify.reset();
    }

    pub fn show_first_page(&self, total: usize, songs: Vec<Song>) {
        {
            let mut state = self.state.borrow_mut();
            state.total = total;
            state.pages.insert(0, songs);
        }
        self.notify.reset();
    }

    pub fn apply_page(&self, page: usize, songs: Vec<Song>) {
        let rows = {
            let mut state = self.state.borrow_mut();
            let rows = state.rows_of_page(page, songs.len());
            state.pages.insert(page, songs);
            if !state.pages.has_page(page) {
                return;
            }
            rows
        };
        self.notify_rows(rows);
    }

    pub fn relay(&self, page: usize, total: usize, songs: Vec<Song>) {
        {
            let mut state = self.state.borrow_mut();
            state.total = total;
            state.pages.invalidate();
            state.pages.insert(page, songs);
            state.selection = Selection::default();
        }
        self.notify.reset();
    }

    pub fn page_failed(&self, page: usize) {
        self.state.borrow_mut().pages.request_failed(page);
    }

    pub fn focus_on(&self, row: usize) {
        self.state.borrow_mut().pages.focus_on(row);
    }

    pub fn claim_pages_around(&self, row: usize) -> Vec<usize> {
        let mut state = self.state.borrow_mut();
        let total = state.total;
        state.pages.claim_pages_around(row, total)
    }

    pub fn claim_page_of(&self, row: usize) -> Option<usize> {
        let mut state = self.state.borrow_mut();
        let page = state.pages.page_of(row);
        state.pages.claim_page(page).then_some(page)
    }

    pub fn change_selection(
        &self,
        index: Option<usize>,
        change: impl FnOnce(&mut Selection, usize),
    ) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let total = state.total;
            if index.is_some_and(|index| index >= total) {
                return;
            }
            let before = state.selection.ranges().clone();
            change(&mut state.selection, total);
            before.symmetric_difference(state.selection.ranges())
        };
        self.notify_ranges(&changed);
    }

    pub fn update_songs(&self, ids: &[String], update: impl Fn(&mut Song)) {
        let ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let changed: Vec<usize> = {
            let mut state = self.state.borrow_mut();
            state
                .pages
                .rows_mut()
                .filter(|(_, song)| ids.contains(song.id.as_str()))
                .map(|(index, song)| {
                    update(song);
                    index
                })
                .collect()
        };
        self.notify_rows(changed);
    }

    pub fn notify_rows(&self, rows: impl IntoIterator<Item = usize>) {
        for row in rows {
            self.notify.row_changed(row);
        }
    }

    fn notify_ranges(&self, ranges: &RowRanges) {
        self.notify_rows(ranges.rows());
    }
}
