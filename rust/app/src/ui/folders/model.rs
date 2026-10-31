use std::cell::RefCell;
use std::ops::Range;

use cadence_core::model::{Folder, FolderId, Song};
use slint::{Model, ModelNotify, ModelTracker};

use crate::ui::page_cache::{PageCache, PageShape};
use crate::ui::rows::notify_row_count;
use crate::ui::tracks::{placeholder_track, track_row};
use crate::ui::{FolderListRow, TrackRow};

pub const SONG_PAGE_SIZE: usize = 100;
const SONG_PAGES: PageShape = PageShape {
    page_size: SONG_PAGE_SIZE,
    capacity: 8,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongPageRequest {
    pub generation: u64,
    pub folder: FolderId,
    pub page: usize,
}

pub struct FirstSongs {
    pub song_total: usize,
    pub songs: Vec<Song>,
}

struct Listing {
    generation: u64,
    folder: FolderId,
    folders: Vec<Folder>,
    song_total: usize,
    songs: PageCache<Song>,
    has_folders: bool,
    has_songs: bool,
}

impl Listing {
    fn empty(generation: u64, folder: FolderId) -> Self {
        Self {
            generation,
            folder,
            folders: Vec::new(),
            song_total: 0,
            songs: PageCache::new(SONG_PAGES),
            has_folders: false,
            has_songs: false,
        }
    }

    fn row_count(&self) -> usize {
        self.folders.len() + self.song_total
    }

    fn adopt_first_songs(&mut self, listed: FirstSongs) -> Range<usize> {
        let FirstSongs { song_total, songs } = listed;
        let song_total = song_total.max(songs.len());
        if self.song_total != song_total {
            self.songs.invalidate();
        }
        self.song_total = song_total;
        self.has_songs = true;
        let first_row = self.folders.len();
        let shown_rows = first_row..first_row + songs.len();
        self.songs.insert(0, songs);
        shown_rows
    }

    fn song_position(&self, row: usize) -> Option<usize> {
        row.checked_sub(self.folders.len())
            .filter(|position| *position < self.song_total)
    }

    fn claim_song_pages_for(&mut self, row: usize) -> Vec<SongPageRequest> {
        let Some(position) = self.song_position(row) else {
            return Vec::new();
        };
        self.songs.focus_on(position);
        self.songs
            .claim_pages_around(position, self.song_total)
            .into_iter()
            .map(|page| SongPageRequest {
                generation: self.generation,
                folder: self.folder.clone(),
                page,
            })
            .collect()
    }
}

fn folder_row(folder: &Folder) -> FolderListRow {
    FolderListRow {
        is_folder: true,
        folder_id: folder.id.as_str().into(),
        name: folder.name.as_str().into(),
        track: TrackRow::default(),
    }
}

fn song_row(track: TrackRow) -> FolderListRow {
    FolderListRow {
        is_folder: false,
        folder_id: "".into(),
        name: track.title.clone(),
        track,
    }
}

pub struct FolderRowsModel {
    state: RefCell<Listing>,
    notify: ModelNotify,
    fetch: Box<dyn Fn(SongPageRequest)>,
}

impl FolderRowsModel {
    pub fn new(fetch: impl Fn(SongPageRequest) + 'static) -> Self {
        Self {
            state: RefCell::new(Listing::empty(0, FolderId::Root)),
            notify: ModelNotify::default(),
            fetch: Box::new(fetch),
        }
    }

    pub fn reset(&self, folder: FolderId) -> u64 {
        let generation = {
            let mut state = self.state.borrow_mut();
            let generation = state.generation + 1;
            *state = Listing::empty(generation, folder);
            generation
        };
        self.notify.reset();
        generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.state.borrow().generation == generation
    }

    pub fn show_folders(&self, generation: u64, folders: Vec<Folder>) {
        if !self.is_current(generation) {
            return;
        }
        let is_same = {
            let mut state = self.state.borrow_mut();
            state.has_folders = true;
            let is_same = state.folders == folders;
            state.folders = folders;
            is_same
        };
        if !is_same {
            self.notify.reset();
        }
    }

    pub fn show_first_songs(&self, generation: u64, listed: FirstSongs) {
        if !self.is_current(generation) {
            return;
        }
        let (before, shown_rows, after) = {
            let mut state = self.state.borrow_mut();
            let before = state.row_count();
            let shown_rows = state.adopt_first_songs(listed);
            (before, shown_rows, state.row_count())
        };
        notify_row_count(&self.notify, before, after);
        for row in shown_rows {
            self.notify.row_changed(row);
        }
    }

    pub fn is_settled(&self) -> bool {
        let state = self.state.borrow();
        state.has_folders && state.has_songs
    }

    pub fn counts(&self) -> (usize, usize) {
        let state = self.state.borrow();
        (state.folders.len(), state.song_total)
    }

    pub fn apply_song_page(&self, request: &SongPageRequest, songs: Vec<Song>) {
        let rows = {
            let mut state = self.state.borrow_mut();
            if state.generation != request.generation {
                return;
            }
            let first_position = state.songs.first_index_of(request.page);
            let first_row = state.folders.len() + first_position;
            let row_count = songs.len();
            state.songs.insert(request.page, songs);
            first_row..first_row + row_count
        };
        for row in rows {
            self.notify.row_changed(row);
        }
    }

    pub fn song_page_failed(&self, request: &SongPageRequest) {
        let mut state = self.state.borrow_mut();
        if state.generation == request.generation {
            state.songs.request_failed(request.page);
        }
    }

    pub fn folder(&self) -> FolderId {
        self.state.borrow().folder.clone()
    }

    fn request_song_pages_for(&self, row: usize) {
        let requests = self.state.borrow_mut().claim_song_pages_for(row);
        for request in requests {
            (self.fetch)(request);
        }
    }

    pub fn song_at(&self, row: usize) -> Option<(usize, Song)> {
        let state = self.state.borrow();
        let position = state.song_position(row)?;
        state
            .songs
            .get(position)
            .map(|song| (position, song.clone()))
    }
}

impl Model for FolderRowsModel {
    type Data = FolderListRow;

    fn row_count(&self) -> usize {
        self.state.borrow().row_count()
    }

    fn row_data(&self, row: usize) -> Option<FolderListRow> {
        self.request_song_pages_for(row);
        let state = self.state.borrow();
        if let Some(folder) = state.folders.get(row) {
            return Some(folder_row(folder));
        }
        let position = state.song_position(row)?;
        let track = state.songs.get(position).map_or_else(
            || placeholder_track(position),
            |song| track_row(song, position),
        );
        Some(song_row(track))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
