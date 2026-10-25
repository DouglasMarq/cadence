use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cadence_core::model::{Album, AlbumListQuery};
use slint::{
    ComponentHandle, Image, Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel,
};

use super::super::{AppWindow, HomeAlbum, HomeShelf, HomeState, update_first_row};
use super::shelf::{Shelf, album_card};
use crate::ui::cover_slots::CoverSlots;

struct ShelfSlot {
    shelf: Shelf,
    albums: Rc<VecModel<HomeAlbum>>,
    generation: Cell<u64>,
    loading: Cell<bool>,
}

impl ShelfSlot {
    fn new(shelf: Shelf) -> Self {
        Self {
            shelf,
            albums: Rc::default(),
            generation: Cell::default(),
            loading: Cell::default(),
        }
    }

    fn next_generation(&self) -> u64 {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        generation
    }

    fn shows_same_albums(&self, albums: &[Album]) -> bool {
        self.albums.row_count() == albums.len()
            && self
                .albums
                .iter()
                .zip(albums)
                .all(|(card, album)| card.id == album.id.as_str())
    }
}

pub struct HomeModel {
    slots: Vec<ShelfSlot>,
    covers: RefCell<CoverSlots<(usize, SharedString)>>,
    server_id: RefCell<Option<String>>,
    notify: ModelNotify,
}

impl Default for HomeModel {
    fn default() -> Self {
        Self {
            slots: Shelf::ALL.iter().copied().map(ShelfSlot::new).collect(),
            covers: RefCell::default(),
            server_id: RefCell::default(),
            notify: ModelNotify::default(),
        }
    }
}

impl HomeModel {
    fn set_loading(&self, shelf: usize, is_loading: bool) {
        let Some(slot) = self.slots.get(shelf) else {
            return;
        };
        if slot.loading.replace(is_loading) != is_loading {
            self.notify.row_changed(shelf);
        }
    }

    fn forget_previous_server(&self, server_id: &str) {
        *self.server_id.borrow_mut() = Some(server_id.to_owned());
        self.covers.borrow_mut().clear();
        for (shelf, slot) in self.slots.iter().enumerate() {
            slot.next_generation();
            slot.albums.clear();
            self.set_loading(shelf, false);
        }
    }

    pub fn visit(&self, server_id: &str) -> Vec<usize> {
        let has_switched_server = self.server_id.borrow().as_deref() != Some(server_id);
        if has_switched_server {
            self.forget_previous_server(server_id);
        }
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.shelf.reloads_on_every_visit() || slot.albums.row_count() == 0)
            .map(|(shelf, _)| shelf)
            .collect()
    }

    pub fn begin_fetch(&self, shelf: usize) -> Option<(u64, AlbumListQuery)> {
        let slot = self.slots.get(shelf)?;
        let generation = slot.next_generation();
        if slot.albums.row_count() == 0 {
            self.set_loading(shelf, true);
        }
        Some((generation, slot.shelf.query()))
    }

    pub fn is_current_fetch(&self, shelf: usize, generation: u64) -> bool {
        self.slots
            .get(shelf)
            .is_some_and(|slot| slot.generation.get() == generation)
    }

    pub fn apply_fetched(&self, shelf: usize, generation: u64, albums: &[Album]) {
        let Some(slot) = self.slots.get(shelf) else {
            return;
        };
        if slot.generation.get() != generation {
            return;
        }
        self.set_loading(shelf, false);
        if !slot.shows_same_albums(albums) {
            self.covers
                .borrow_mut()
                .forget(|(shown_shelf, _)| *shown_shelf == shelf);
            slot.albums
                .set_vec(albums.iter().map(album_card).collect::<Vec<_>>());
        }
    }

    pub fn fetch_failed(&self, shelf: usize, generation: u64) {
        if self.is_current_fetch(shelf, generation) {
            self.set_loading(shelf, false);
        }
    }

    pub fn request_cover(&self, shelf: usize, index: usize) -> Option<(SharedString, String)> {
        let slot = self.slots.get(shelf)?;
        let card = slot.albums.row_data(index)?;
        if card.has_cover || card.cover_id.is_empty() {
            return None;
        }
        self.covers
            .borrow_mut()
            .claim((shelf, card.id.clone()))
            .then(|| (card.id, card.cover_id.to_string()))
    }

    pub fn set_cover(&self, shelf: usize, album_id: &SharedString, cover: Option<Image>) {
        if !self.covers.borrow_mut().finish(&(shelf, album_id.clone())) {
            return;
        }
        let Some(cover) = cover else {
            return;
        };
        if !self.has_card(shelf, album_id) {
            return;
        }
        self.update_card(shelf, album_id, |card| {
            card.cover = cover;
            card.has_cover = true;
        });
        self.release_oldest_covers(shelf, album_id);
    }

    fn release_oldest_covers(&self, shelf: usize, album_id: &SharedString) {
        let evicted = self.covers.borrow_mut().show((shelf, album_id.clone()));
        for (evicted_shelf, evicted_id) in evicted {
            self.update_card(evicted_shelf, &evicted_id, |card| {
                card.cover = Image::default();
                card.has_cover = false;
            });
        }
    }

    fn has_card(&self, shelf: usize, album_id: &SharedString) -> bool {
        self.slots
            .get(shelf)
            .is_some_and(|slot| slot.albums.iter().any(|card| card.id == *album_id))
    }

    fn update_card(
        &self,
        shelf: usize,
        album_id: &SharedString,
        change: impl FnOnce(&mut HomeAlbum),
    ) {
        if let Some(slot) = self.slots.get(shelf) {
            update_first_row(&slot.albums, |card| card.id == *album_id, change);
        }
    }

    #[cfg(test)]
    pub fn cards(&self, shelf: usize) -> Vec<HomeAlbum> {
        self.slots
            .get(shelf)
            .map(|slot| slot.albums.iter().collect())
            .unwrap_or_default()
    }
}

impl Model for HomeModel {
    type Data = HomeShelf;

    fn row_count(&self) -> usize {
        self.slots.len()
    }

    fn row_data(&self, row: usize) -> Option<HomeShelf> {
        let slot = self.slots.get(row)?;
        Some(HomeShelf {
            title: slot.shelf.title().into(),
            albums: ModelRc::from(Rc::clone(&slot.albums)),
            is_loading: slot.loading.get(),
            is_refreshable: slot.shelf.is_refreshable(),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn with_home_model<Output>(
    window: &AppWindow,
    action: impl FnOnce(&HomeModel) -> Output,
) -> Option<Output> {
    let shelves = window.global::<HomeState>().get_shelves();
    shelves.as_any().downcast_ref::<HomeModel>().map(action)
}

#[cfg(test)]
mod tests {
    use super::super::test_albums::{album, albums, one_pixel_image};
    use super::*;
    use crate::ui::cover_slots::MAX_SHOWN_COVERS;

    fn fetch_generation(model: &HomeModel, shelf: usize) -> u64 {
        model
            .begin_fetch(shelf)
            .map(|(generation, _)| generation)
            .unwrap_or_default()
    }

    #[test]
    fn stale_responses_are_ignored() {
        let model = HomeModel::default();
        assert_eq!(model.visit("s1"), [0, 1, 2, 3]);
        let first = model.begin_fetch(0).map(|(generation, _)| generation);
        let second = model.begin_fetch(0).map(|(generation, _)| generation);
        assert!(model.row_data(0).is_some_and(|shelf| shelf.is_loading));

        assert!(!model.is_current_fetch(0, first.unwrap_or_default()));
        model.apply_fetched(0, first.unwrap_or_default(), &albums(&["old"]));
        assert!(model.cards(0).is_empty());
        assert!(model.is_current_fetch(0, second.unwrap_or_default()));
        model.apply_fetched(0, second.unwrap_or_default(), &albums(&["a", "b"]));
        assert!(model.row_data(0).is_some_and(|shelf| !shelf.is_loading));
        assert_eq!(model.cards(0).len(), 2);
    }

    #[test]
    fn random_is_kept_between_visits_until_the_server_changes() {
        let model = HomeModel::default();
        model.visit("s1");
        for shelf in 0..4 {
            let generation = fetch_generation(&model, shelf);
            model.apply_fetched(shelf, generation, &albums(&["a"]));
        }
        assert_eq!(model.visit("s1"), [0, 1, 2]);
        assert_eq!(model.visit("s2"), [0, 1, 2, 3]);
        assert!(model.cards(3).is_empty());
    }

    #[test]
    fn covers_are_requested_once_and_kept_when_rows_do_not_change() {
        let model = HomeModel::default();
        model.visit("s1");
        let generation = fetch_generation(&model, 0);
        model.apply_fetched(0, generation, &albums(&["a", "b"]));

        let wanted = model.request_cover(0, 1);
        assert_eq!(wanted, Some(("b".into(), "al-b".into())));
        assert_eq!(model.request_cover(0, 1), None);
        model.set_cover(0, &"b".into(), Some(one_pixel_image()));
        assert!(model.cards(0)[1].has_cover);
        assert_eq!(model.request_cover(0, 1), None);

        let generation = fetch_generation(&model, 0);
        model.apply_fetched(0, generation, &albums(&["a", "b"]));
        assert!(model.cards(0)[1].has_cover);

        let generation = fetch_generation(&model, 0);
        model.apply_fetched(0, generation, &albums(&["c", "b"]));
        assert!(!model.cards(0)[1].has_cover);
    }

    #[test]
    fn decoded_covers_stay_bounded() {
        let model = HomeModel::default();
        model.visit("s1");
        let ids: Vec<String> = (0..MAX_SHOWN_COVERS + 5)
            .map(|index| format!("a{index}"))
            .collect();
        let list: Vec<Album> = ids.iter().map(|id| album(id)).collect();
        let generation = fetch_generation(&model, 1);
        model.apply_fetched(1, generation, &list);
        for index in 0..list.len() {
            if let Some((album_id, _)) = model.request_cover(1, index) {
                model.set_cover(1, &album_id, Some(one_pixel_image()));
            }
        }
        let cards = model.cards(1);
        assert_eq!(
            cards.iter().filter(|card| card.has_cover).count(),
            MAX_SHOWN_COVERS
        );
        assert!(!cards[0].has_cover && cards[list.len() - 1].has_cover);
        assert!(model.request_cover(1, 0).is_some());
    }

    #[test]
    fn failed_covers_can_be_requested_again() {
        let model = HomeModel::default();
        model.visit("s1");
        let generation = fetch_generation(&model, 2);
        model.apply_fetched(2, generation, &albums(&["a"]));
        let wanted = model.request_cover(2, 0).map(|(album_id, _)| album_id);
        model.set_cover(2, &wanted.unwrap_or_default(), None);
        assert!(!model.cards(2)[0].has_cover);
        assert!(model.request_cover(2, 0).is_some());
    }
}
