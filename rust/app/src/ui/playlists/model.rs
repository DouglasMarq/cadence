use std::cell::{Cell, RefCell};

use cadence_core::model::Playlist;
use slint::{Image, Model, ModelNotify, ModelTracker};

use crate::ui::cover_lru::CoverLru;
use crate::ui::rows::notify_row_count;
use crate::ui::{PlaylistRow, format_duration};

const COVER_CAPACITY: usize = 48;

fn playlist_row(playlist: &Playlist) -> PlaylistRow {
    PlaylistRow {
        id: playlist.id.as_str().into(),
        name: playlist.name.as_str().into(),
        songs: playlist.song_count.to_string().into(),
        duration: format_duration(playlist.duration_ms).into(),
        owner: playlist.owner.as_deref().unwrap_or_default().into(),
        cover: Image::default(),
        has_cover: false,
    }
}

#[derive(Clone, PartialEq)]
struct PlaylistEntry {
    row: PlaylistRow,
    cover_id: Option<String>,
}

pub struct PlaylistsModel {
    entries: RefCell<Vec<PlaylistEntry>>,
    covers: RefCell<CoverLru<String, Image>>,
    generation: Cell<u64>,
    notify: ModelNotify,
    fetch_cover: Box<dyn Fn(String)>,
}

impl PlaylistsModel {
    pub fn new(fetch_cover: impl Fn(String) + 'static) -> Self {
        Self {
            entries: RefCell::default(),
            covers: RefCell::new(CoverLru::new(COVER_CAPACITY)),
            generation: Cell::new(0),
            notify: ModelNotify::default(),
            fetch_cover: Box::new(fetch_cover),
        }
    }

    pub fn begin(&self) -> u64 {
        self.generation.set(self.generation.get() + 1);
        self.generation.get()
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation
    }

    pub fn set_playlists(&self, generation: u64, playlists: &[Playlist]) {
        if !self.is_current(generation) {
            return;
        }
        let entries: Vec<PlaylistEntry> = playlists
            .iter()
            .map(|playlist| PlaylistEntry {
                row: playlist_row(playlist),
                cover_id: playlist.cover_art_id.clone(),
            })
            .collect();
        let changed: Vec<usize> = entries
            .iter()
            .zip(self.entries.borrow().iter())
            .enumerate()
            .filter(|(_, (entry, shown))| entry != shown)
            .map(|(row, _)| row)
            .collect();
        let after = entries.len();
        let before = self.entries.replace(entries).len();
        notify_row_count(&self.notify, before, after);
        for row in changed {
            self.notify.row_changed(row);
        }
    }

    pub fn apply_cover(&self, cover_id: String, image: Option<Image>) {
        let Some(image) = image else {
            self.covers.borrow_mut().retry_later(cover_id.as_str());
            return;
        };
        let changed: Vec<usize> = self
            .entries
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.cover_id.as_deref() == Some(cover_id.as_str()))
            .map(|(index, _)| index)
            .collect();
        self.covers.borrow_mut().insert(cover_id, image);
        for row in changed {
            self.notify.row_changed(row);
        }
    }

    fn with_cover(&self, mut row: PlaylistRow, cover_id: String) -> PlaylistRow {
        let cached = self.covers.borrow().peek(cover_id.as_str());
        match cached {
            Some(image) => {
                row.cover = image;
                row.has_cover = true;
            }
            None => self.request_cover(cover_id),
        }
        row
    }

    fn request_cover(&self, cover_id: String) {
        let first_request = self.covers.borrow_mut().begin(cover_id.as_str());
        if first_request {
            (self.fetch_cover)(cover_id);
        }
    }
}

impl Model for PlaylistsModel {
    type Data = PlaylistRow;

    fn row_count(&self) -> usize {
        self.entries.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<PlaylistRow> {
        let entry = self.entries.borrow().get(row).cloned()?;
        let Some(cover_id) = entry.cover_id else {
            return Some(entry.row);
        };
        Some(self.with_cover(entry.row, cover_id))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn playlist(id: &str, cover: Option<&str>) -> Playlist {
        Playlist {
            id: id.into(),
            server_id: "demo".into(),
            name: id.into(),
            comment: None,
            owner: Some("me".into()),
            is_public: false,
            song_count: 3,
            duration_ms: 185_000,
            cover_art_id: cover.map(Into::into),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn rows_map_playlists() {
        let row = playlist_row(&playlist("a", None));
        assert_eq!(row.songs, "3");
        assert_eq!(row.duration, "3:05");
        assert_eq!(row.owner, "me");
    }

    #[test]
    fn given_a_cover_that_never_arrived_when_the_row_is_read_again_then_it_is_requested_again() {
        let requested = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&requested);
        let model = PlaylistsModel::new(move |cover_id| sink.borrow_mut().push(cover_id));
        let generation = model.begin();
        model.set_playlists(generation, &[playlist("a", Some("c1"))]);
        model.row_data(0);

        model.apply_cover("c1".into(), None);
        model.row_data(0);

        assert_eq!(*requested.borrow(), ["c1", "c1"]);
    }

    #[test]
    fn given_a_refresh_when_it_differs_then_rows_change_in_place() {
        let model = PlaylistsModel::new(|_cover_id| {});
        let generation = model.begin();
        model.set_playlists(generation, &[playlist("a", None), playlist("b", None)]);

        model.set_playlists(generation, &[playlist("a", None), playlist("c", None)]);
        assert_eq!(model.row_data(1).map(|row| row.name), Some("c".into()));

        model.set_playlists(generation, &[playlist("a", None)]);
        assert_eq!(model.row_count(), 1);
    }

    #[test]
    fn covers_are_requested_once_and_stale_loads_ignored() {
        let requested = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&requested);
        let model = PlaylistsModel::new(move |cover_id| sink.borrow_mut().push(cover_id));
        let stale = model.begin();
        let current = model.begin();
        assert!(!model.is_current(stale));
        model.set_playlists(stale, &[playlist("old", None)]);
        model.set_playlists(current, &[playlist("a", Some("c1")), playlist("b", None)]);
        assert_eq!(model.row_count(), 2);

        model.row_data(0);
        model.row_data(0);
        model.row_data(1);
        assert_eq!(*requested.borrow(), ["c1"]);

        model.apply_cover("c1".into(), Some(Image::default()));
        assert!(model.row_data(0).is_some_and(|row| row.has_cover));
        assert!(model.row_data(1).is_some_and(|row| !row.has_cover));
    }
}
