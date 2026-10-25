use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

use slint::Image;

use super::card::{CardData, CardPage, CardSource};
use super::cover_cache::{cached_cover, has_cached_cover};
use crate::ui::Card;
use crate::ui::grid_pages::{GridPages, WantedPage};
use crate::ui::page_cache::{PageShape, changed_total};

pub const GRID_PAGE: usize = 60;
const CACHED_PAGES: usize = 16;
const GRID_PAGES: PageShape = PageShape {
    page_size: GRID_PAGE,
    capacity: CACHED_PAGES,
};

pub enum GridRequest {
    Page {
        generation: u64,
        source: CardSource,
        page: usize,
    },
    Cover {
        cover_id: String,
    },
}

pub struct RowChange {
    pub old_rows: usize,
    pub new_rows: usize,
    pub changed: Range<usize>,
}

pub struct GridState {
    grid: GridPages<CardSource, CardData>,
    reported_total: Option<usize>,
    covers_requested: HashSet<String>,
    overrides: HashMap<String, bool>,
}

impl Default for GridState {
    fn default() -> Self {
        Self {
            grid: GridPages::new(GRID_PAGES),
            reported_total: None,
            covers_requested: HashSet::new(),
            overrides: HashMap::new(),
        }
    }
}

impl GridState {
    pub fn is_current(&self, generation: u64) -> bool {
        self.grid.is_current(generation)
    }

    pub fn source(&self) -> Option<&CardSource> {
        self.grid.source()
    }

    pub fn columns(&self) -> usize {
        self.grid.columns()
    }

    pub fn set_columns(&mut self, columns: usize) {
        self.grid.set_columns(columns);
    }

    pub fn rows(&self) -> usize {
        self.grid.row_count()
    }

    fn card_with_id(&self, id: &str) -> Option<(usize, &CardData)> {
        self.grid.cards().find(|(_, card)| card.id == id)
    }

    pub fn restart(&mut self, source: Option<CardSource>) {
        self.grid.restart(source);
        self.reported_total = None;
        self.covers_requested.clear();
        self.overrides.clear();
    }

    pub fn claim_first_page(&mut self) -> Option<GridRequest> {
        let source = self.grid.source()?.clone();
        self.grid.mark_requested(0);
        Some(GridRequest::Page {
            generation: self.grid.generation(),
            source,
            page: 0,
        })
    }

    pub fn reported_total_change(&self, total: Option<usize>) -> Option<usize> {
        self.reported_total
            .and_then(|known| changed_total(known, total))
    }

    pub fn row_change_after(&self, page: usize, listed: &CardPage) -> RowChange {
        let start = self.grid.first_index_of(page);
        let end = start + listed.cards.len();
        let columns = self.grid.columns();
        let new_total = self.total_after_page(listed, end);
        RowChange {
            old_rows: self.rows(),
            new_rows: new_total.div_ceil(columns),
            changed: start / columns..end.div_ceil(columns),
        }
    }

    pub fn insert_cards(&mut self, page: usize, listed: CardPage) {
        let end = self.grid.first_index_of(page) + listed.cards.len();
        let total = self.total_after_page(&listed, end);
        self.grid.set_total(total);
        self.reported_total = listed.total.or(self.reported_total);
        self.grid.insert(page, listed.cards);
    }

    pub fn relay_cards(&mut self, page: usize, cards: Vec<CardData>, total: usize) {
        tracing::debug!(
            total,
            "the server card list changed size, reloading the cached pages"
        );
        self.reported_total = Some(total);
        self.grid.relay(page, cards, total);
    }

    fn total_after_page(&self, listed: &CardPage, end: usize) -> usize {
        match listed.total {
            Some(total) => total,
            None if listed.cards.len() == GRID_PAGE => self.grid.total().max(end + 1),
            None => end,
        }
    }

    pub fn is_waiting_for(&self, generation: u64, page: usize) -> bool {
        self.grid.is_waiting_for(generation, page)
    }

    pub fn page_failed(&mut self, generation: u64, page: usize) {
        self.grid.page_failed(generation, page);
    }

    pub fn cover_loaded(&mut self, cover_id: &str) {
        self.covers_requested.remove(cover_id);
    }

    pub fn rows_with_cover(&self, cover_id: &str) -> BTreeSet<usize> {
        self.grid.rows_of(
            self.grid
                .cards()
                .filter(|(_, card)| card.cover_id.as_deref() == Some(cover_id))
                .map(|(index, _)| index),
        )
    }

    pub fn is_favorite(&self, id: &str) -> Option<bool> {
        let (_, card) = self.card_with_id(id)?;
        Some(self.overrides.get(id).copied().unwrap_or(card.is_favorite))
    }

    pub fn override_favorite(&mut self, id: &str, is_favorite: bool) {
        self.overrides.insert(id.to_owned(), is_favorite);
    }

    pub fn row_of_card(&self, id: &str) -> Option<usize> {
        self.card_with_id(id)
            .map(|(index, _)| self.grid.row_of(index))
    }

    pub fn claim_row(&mut self, row: usize) -> Vec<GridRequest> {
        let mut requests: Vec<GridRequest> = self
            .grid
            .claim_pages_for_row(row)
            .into_iter()
            .map(page_request)
            .collect();
        requests.extend(self.claim_row_covers(row));
        requests
    }

    fn claim_row_covers(&mut self, row: usize) -> Vec<GridRequest> {
        let Some(indexes) = self.grid.row_indexes(row) else {
            return Vec::new();
        };
        let cover_ids: Vec<String> = indexes
            .filter_map(|index| self.grid.card(index)?.cover_id.clone())
            .filter(|cover_id| !has_cached_cover(cover_id))
            .collect();
        cover_ids
            .into_iter()
            .filter(|cover_id| self.covers_requested.insert(cover_id.clone()))
            .map(|cover_id| GridRequest::Cover { cover_id })
            .collect()
    }

    pub fn cards_for_row(&self, row: usize) -> Option<Vec<Card>> {
        let indexes = self.grid.row_indexes(row)?;
        Some(indexes.map(|index| self.card_at(index)).collect())
    }

    fn card_at(&self, index: usize) -> Card {
        let Some(data) = self.grid.card(index) else {
            return Card::default();
        };
        let cover = data.cover_id.as_deref().and_then(cached_cover);
        let is_favorite = self
            .overrides
            .get(&data.id)
            .copied()
            .unwrap_or(data.is_favorite);
        loaded_card(data, cover, is_favorite)
    }
}

fn page_request(wanted: WantedPage<CardSource>) -> GridRequest {
    GridRequest::Page {
        generation: wanted.generation,
        source: wanted.source,
        page: wanted.page,
    }
}

fn loaded_card(data: &CardData, cover: Option<Image>, is_favorite: bool) -> Card {
    Card {
        id: data.id.as_str().into(),
        title: data.title.as_str().into(),
        subtitle: data.subtitle.as_str().into(),
        has_cover: cover.is_some(),
        cover: cover.unwrap_or_default(),
        is_favorite,
        is_loaded: true,
    }
}
