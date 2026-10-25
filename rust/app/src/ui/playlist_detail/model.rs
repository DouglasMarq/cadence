use std::cell::RefCell;

use cadence_core::model::{Page, Song};
use slint::{Model, ModelTracker};

use super::pages::PLAYLIST_PAGES;
use super::rows::{placeholder_row, song_row};
use super::targets::TargetSnapshot;
use crate::ui::PlaylistSongRow;
use crate::ui::page_cache::changed_total;
use crate::ui::tracks::{PagedSongs, Selection};

pub struct PageRequest {
    pub generation: u64,
    pub revision: u64,
    pub playlist_id: String,
    pub page: usize,
}

#[derive(Default)]
struct Listing {
    generation: u64,
    revision: u64,
    playlist_id: String,
}

pub struct PlaylistSongsModel {
    listing: RefCell<Listing>,
    songs: PagedSongs,
    fetch: Box<dyn Fn(PageRequest)>,
}

impl PlaylistSongsModel {
    pub fn new(fetch: impl Fn(PageRequest) + 'static) -> Self {
        Self {
            listing: RefCell::default(),
            songs: PagedSongs::new(PLAYLIST_PAGES),
            fetch: Box::new(fetch),
        }
    }

    pub fn reset(&self, generation: u64, playlist_id: &str) {
        *self.listing.borrow_mut() = Listing {
            generation,
            playlist_id: playlist_id.to_owned(),
            ..Listing::default()
        };
        self.songs.reset();
    }

    pub fn is_showing(&self, generation: u64) -> bool {
        self.listing.borrow().generation == generation
    }

    pub fn is_current(&self, request: &PageRequest) -> bool {
        let listing = self.listing.borrow();
        (listing.generation, listing.revision) == (request.generation, request.revision)
    }

    pub fn apply_first_page(&self, generation: u64, total: usize, songs: Vec<Song>) {
        if self.is_showing(generation) {
            self.songs.show_first_page(total.max(songs.len()), songs);
        }
    }

    pub fn apply_page(&self, generation: u64, page: usize, songs: Vec<Song>) {
        if self.is_showing(generation) {
            self.songs.apply_page(page, songs);
        }
    }

    pub fn reported_total_change(
        &self,
        request: &PageRequest,
        reported_total: Option<usize>,
    ) -> Option<usize> {
        if !self.is_current(request) {
            return None;
        }
        changed_total(self.songs.total(), reported_total)
    }

    pub fn apply_requested_page(&self, request: &PageRequest, listed: Page<Song>) {
        if !self.is_current(request) {
            return;
        }
        match self.reported_total_change(request, listed.total_count) {
            Some(total) => self.relay_rows(request.page, total, listed.items),
            None => self.apply_page(request.generation, request.page, listed.items),
        }
    }

    fn relay_rows(&self, page: usize, total: usize, songs: Vec<Song>) {
        tracing::debug!(
            total,
            "the playlist changed size on the server, reloading its pages"
        );
        self.listing.borrow_mut().revision += 1;
        self.songs.relay(page, total, songs);
    }

    pub fn page_failed(&self, request: &PageRequest) {
        if self.is_current(request) {
            self.songs.page_failed(request.page);
        }
    }

    pub fn playlist_id(&self) -> String {
        self.listing.borrow().playlist_id.clone()
    }

    pub fn total(&self) -> usize {
        self.songs.total()
    }

    pub fn song_id_at(&self, index: usize) -> Option<String> {
        self.songs.song_id(index)
    }

    pub fn snapshot(&self, indexes: &[usize]) -> TargetSnapshot {
        let songs = self.songs.read();
        TargetSnapshot {
            playlist_id: self.playlist_id(),
            rows: indexes
                .iter()
                .filter(|index| **index < songs.total())
                .map(|&index| (index, songs.song(index).cloned()))
                .collect(),
        }
    }

    fn request_page_of(&self, row: usize) {
        let Some(page) = self.songs.claim_page_of(row) else {
            return;
        };
        let request = {
            let listing = self.listing.borrow();
            PageRequest {
                generation: listing.generation,
                revision: listing.revision,
                playlist_id: listing.playlist_id.clone(),
                page,
            }
        };
        (self.fetch)(request);
    }
}

impl PlaylistSongsModel {
    pub fn click(&self, index: usize, is_toggling: bool, is_extending: bool) {
        self.songs.change_selection(Some(index), |selection, _| {
            selection.click(index, is_toggling, is_extending);
        });
    }

    pub fn select_all(&self) {
        self.songs.change_selection(None, Selection::select_all);
    }

    pub fn clear_selection(&self) {
        self.songs
            .change_selection(None, |selection, _| selection.clear());
    }

    pub fn selected_count(&self) -> usize {
        self.songs.selected_count()
    }

    pub fn selected_indexes(&self) -> Vec<usize> {
        self.songs.read().selection().indexes()
    }

    pub fn targets(&self, index: usize) -> Vec<usize> {
        self.songs.read().selection().targets(index)
    }
}

impl Model for PlaylistSongsModel {
    type Data = PlaylistSongRow;

    fn row_count(&self) -> usize {
        self.songs.total()
    }

    fn row_data(&self, row: usize) -> Option<PlaylistSongRow> {
        if row >= self.songs.total() {
            return None;
        }
        self.songs.focus_on(row);
        if let Some(loaded) = self.loaded_row(row) {
            return Some(loaded);
        }
        self.request_page_of(row);
        Some(placeholder_row(row, self.songs.read().is_selected(row)))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        self.songs.tracker()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl PlaylistSongsModel {
    fn loaded_row(&self, row: usize) -> Option<PlaylistSongRow> {
        let songs = self.songs.read();
        let song = songs.song(row)?;
        Some(song_row(song, row, songs.is_selected(row)))
    }
}
