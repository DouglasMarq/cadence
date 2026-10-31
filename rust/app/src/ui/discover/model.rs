use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cadence_youtube::music::{MusicShelf, MusicSong, ShelfItem};
use slint::{ComponentHandle, Image, Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::cards::{card_of, song_row, thumbnail_of};
use super::mixes::MixState;
use super::page::Page;
use super::sections::Sections;
use crate::discover::FeedRequest;
use crate::ui::cover_slots::CoverSlots;
use crate::ui::{
    AppWindow, DiscoverCard, DiscoverShelf, DiscoverSong, DiscoverState, update_first_row,
};

type CardKey = (usize, String);

pub struct CoverRequest {
    pub generation: u64,
    pub url: String,
}

struct ShelfSlot {
    title: String,
    items: Vec<ShelfItem>,
    cards: Rc<VecModel<DiscoverCard>>,
}

#[derive(Default)]
pub struct DiscoverModel {
    slots: RefCell<Vec<ShelfSlot>>,
    notify: ModelNotify,
    shown_request: RefCell<Option<FeedRequest>>,
    shown_mix_revision: Cell<u64>,
    pub mixes: RefCell<MixState>,
    shelf_generation: Cell<u64>,
    card_covers: RefCell<CoverSlots<CardKey>>,
    pub sections: RefCell<Sections>,
    pub page: RefCell<Page>,
    pub songs: Rc<VecModel<DiscoverSong>>,
    song_covers: RefCell<CoverSlots<usize>>,
    song_generation: Cell<u64>,
}

impl DiscoverModel {
    pub fn show_shelves(&self, request: Option<&FeedRequest>, shelves: &[MusicShelf]) {
        let mix_revision = self.mixes.borrow().revision();
        if self.shown_request.borrow().as_ref() == request
            && self.shown_mix_revision.get() == mix_revision
            && self.shows_same_shelves(shelves)
        {
            return;
        }
        *self.shown_request.borrow_mut() = request.cloned();
        self.shown_mix_revision.set(mix_revision);
        self.shelf_generation.set(self.shelf_generation.get() + 1);
        self.card_covers.borrow_mut().clear();
        *self.slots.borrow_mut() = shelves.iter().map(slot_of).collect();
        self.notify.reset();
    }

    fn shows_same_shelves(&self, shelves: &[MusicShelf]) -> bool {
        let slots = self.slots.borrow();
        slots.len() == shelves.len()
            && slots
                .iter()
                .zip(shelves)
                .all(|(slot, shelf)| slot.title == shelf.title && slot.items == shelf.items)
    }

    pub fn forget_shown_shelves(&self) {
        self.show_shelves(None, &[]);
    }

    pub fn item(&self, shelf: usize, card: usize) -> Option<ShelfItem> {
        self.slots.borrow().get(shelf)?.items.get(card).cloned()
    }

    pub fn shelf_songs_from(&self, shelf: usize, card: usize) -> Option<(Vec<MusicSong>, usize)> {
        let slots = self.slots.borrow();
        let items = &slots.get(shelf)?.items;
        let start = items
            .iter()
            .take(card)
            .filter(|item| matches!(item, ShelfItem::Song(_)))
            .count();
        let songs = items
            .iter()
            .filter_map(|item| match item {
                ShelfItem::Song(song) => Some(song.clone()),
                ShelfItem::Collection(_) => None,
            })
            .collect();
        matches!(items.get(card), Some(ShelfItem::Song(_))).then_some((songs, start))
    }

    pub fn request_card_cover(&self, shelf: usize, card: usize) -> Option<(CoverRequest, CardKey)> {
        let slots = self.slots.borrow();
        let slot = slots.get(shelf)?;
        let shown = slot.cards.row_data(card)?;
        let url = slot.items.get(card).and_then(thumbnail_of)?.to_owned();
        if shown.has_cover {
            return None;
        }
        let key = (shelf, shown.id.to_string());
        let claimed = self.card_covers.borrow_mut().claim(key.clone());
        let request = CoverRequest {
            generation: self.shelf_generation.get(),
            url,
        };
        claimed.then_some((request, key))
    }

    pub fn set_card_cover(&self, generation: u64, key: CardKey, cover: Option<Image>) {
        if generation != self.shelf_generation.get() || !self.card_covers.borrow_mut().finish(&key)
        {
            return;
        }
        let Some(cover) = cover else {
            return;
        };
        let (shelf, card_id) = &key;
        if !self.has_card(*shelf, card_id) {
            return;
        }
        self.update_card(*shelf, card_id, Some(cover));
        let evicted = self.card_covers.borrow_mut().show(key);
        for (evicted_shelf, evicted_id) in evicted {
            self.update_card(evicted_shelf, &evicted_id, None);
        }
    }

    pub fn set_card_visible(&self, shelf: usize, card: usize, is_visible: bool) {
        let card_id = self
            .slots
            .borrow()
            .get(shelf)
            .and_then(|slot| slot.cards.row_data(card))
            .map(|shown| shown.id.to_string());
        let Some(card_id) = card_id else {
            return;
        };
        let evicted = self
            .card_covers
            .borrow_mut()
            .set_visible((shelf, card_id), is_visible);
        for (evicted_shelf, evicted_id) in evicted {
            self.update_card(evicted_shelf, &evicted_id, None);
        }
    }

    fn has_card(&self, shelf: usize, card_id: &str) -> bool {
        self.slots
            .borrow()
            .get(shelf)
            .is_some_and(|slot| slot.cards.iter().any(|card| card.id == card_id))
    }

    fn update_card(&self, shelf: usize, card_id: &str, cover: Option<Image>) {
        let slots = self.slots.borrow();
        let Some(slot) = slots.get(shelf) else {
            return;
        };
        update_first_row(
            &slot.cards,
            |card| card.id == card_id,
            |card| {
                card.has_cover = cover.is_some();
                card.cover = cover.unwrap_or_default();
            },
        );
    }

    pub fn show_page_songs(&self) {
        self.song_generation.set(self.song_generation.get() + 1);
        self.song_covers.borrow_mut().forget_covers();
        let rows: Vec<DiscoverSong> = self
            .page
            .borrow()
            .songs
            .iter()
            .enumerate()
            .map(|(position, song)| song_row(position, song))
            .collect();
        self.songs.set_vec(rows);
    }

    pub fn page_song(&self, index: usize) -> Option<MusicSong> {
        self.page.borrow().songs.get(index).cloned()
    }

    pub fn request_song_cover(&self, index: usize) -> Option<CoverRequest> {
        let row = self.songs.row_data(index)?;
        let url = self.page.borrow().songs.get(index)?.thumbnail_url.clone()?;
        if row.has_cover || !self.song_covers.borrow_mut().claim(index) {
            return None;
        }
        Some(CoverRequest {
            generation: self.song_generation.get(),
            url,
        })
    }

    pub fn set_song_cover(&self, generation: u64, index: usize, cover: Option<Image>) {
        if generation != self.song_generation.get() || !self.song_covers.borrow_mut().finish(&index)
        {
            return;
        }
        let Some(cover) = cover else {
            return;
        };
        self.update_song(index, Some(cover));
        let evicted = self.song_covers.borrow_mut().show(index);
        for evicted_index in evicted {
            self.update_song(evicted_index, None);
        }
    }

    pub fn show_song_rows(&self, rows: std::ops::Range<usize>) {
        let evicted = self.song_covers.borrow_mut().show_only(rows);
        for evicted_index in evicted {
            self.update_song(evicted_index, None);
        }
    }

    fn update_song(&self, index: usize, cover: Option<Image>) {
        let Some(mut row) = self.songs.row_data(index) else {
            return;
        };
        row.has_cover = cover.is_some();
        row.cover = cover.unwrap_or_default();
        self.songs.set_row_data(index, row);
    }

    #[cfg(test)]
    pub fn cards(&self, shelf: usize) -> Vec<DiscoverCard> {
        self.slots
            .borrow()
            .get(shelf)
            .map(|slot| slot.cards.iter().collect())
            .unwrap_or_default()
    }
}

fn slot_of(shelf: &MusicShelf) -> ShelfSlot {
    ShelfSlot {
        title: shelf.title.clone(),
        items: shelf.items.clone(),
        cards: Rc::new(VecModel::from(
            shelf.items.iter().map(card_of).collect::<Vec<_>>(),
        )),
    }
}

impl Model for DiscoverModel {
    type Data = DiscoverShelf;

    fn row_count(&self) -> usize {
        self.slots.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<DiscoverShelf> {
        let slots = self.slots.borrow();
        let slot = slots.get(row)?;
        Some(DiscoverShelf {
            title: slot.title.as_str().into(),
            cards: ModelRc::from(Rc::clone(&slot.cards)),
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn with_discover<Output>(
    window: &AppWindow,
    action: impl FnOnce(&DiscoverModel) -> Output,
) -> Option<Output> {
    let shelves = window.global::<DiscoverState>().get_shelves();
    shelves.as_any().downcast_ref::<DiscoverModel>().map(action)
}
