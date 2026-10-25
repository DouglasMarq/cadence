use std::collections::BTreeSet;
use std::ops::Range;

use super::page_cache::{PageCache, PageShape};

#[derive(Debug, Clone, PartialEq)]
pub struct WantedPage<Source> {
    pub generation: u64,
    pub source: Source,
    pub page: usize,
}

pub struct GridPages<Source, Card> {
    shape: PageShape,
    generation: u64,
    source: Option<Source>,
    columns: usize,
    total: usize,
    pages: PageCache<Card>,
}

impl<Source: Clone, Card> GridPages<Source, Card> {
    pub fn new(shape: PageShape) -> Self {
        Self {
            shape,
            generation: 0,
            source: None,
            columns: 1,
            total: 0,
            pages: PageCache::new(shape),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total;
    }

    pub fn row_count(&self) -> usize {
        self.total.div_ceil(self.columns)
    }

    pub fn row_of(&self, index: usize) -> usize {
        index / self.columns
    }

    pub fn rows_of(&self, indexes: impl IntoIterator<Item = usize>) -> BTreeSet<usize> {
        indexes
            .into_iter()
            .map(|index| self.row_of(index))
            .collect()
    }

    pub fn row_indexes(&self, row: usize) -> Option<Range<usize>> {
        let start = row.saturating_mul(self.columns);
        if start >= self.total {
            return None;
        }
        Some(start..start.saturating_add(self.columns).min(self.total))
    }

    pub fn restart(&mut self, source: Option<Source>) -> u64 {
        self.generation += 1;
        self.source = source;
        self.total = 0;
        self.pages = PageCache::new(self.shape);
        self.generation
    }

    pub fn card(&self, index: usize) -> Option<&Card> {
        self.pages.get(index)
    }

    pub fn cards(&self) -> impl Iterator<Item = (usize, &Card)> {
        self.pages.rows()
    }

    pub fn first_index_of(&self, page: usize) -> usize {
        self.pages.first_index_of(page)
    }

    pub fn insert(&mut self, page: usize, cards: Vec<Card>) {
        self.pages.insert(page, cards);
    }

    pub fn relay(&mut self, page: usize, cards: Vec<Card>, total: usize) {
        self.generation += 1;
        self.total = total;
        self.pages.invalidate();
        self.pages.insert(page, cards);
    }

    pub fn is_waiting_for(&self, generation: u64, page: usize) -> bool {
        self.is_current(generation) && self.pages.is_requested(page)
    }

    pub fn mark_requested(&mut self, page: usize) {
        self.pages.mark_requested(page);
    }

    pub fn page_failed(&mut self, generation: u64, page: usize) {
        if self.is_current(generation) {
            self.pages.request_failed(page);
        }
    }

    pub fn claim_pages_for_row(&mut self, row: usize) -> Vec<WantedPage<Source>> {
        let Some(indexes) = self.row_indexes(row) else {
            return Vec::new();
        };
        let Some(source) = self.source.clone() else {
            return Vec::new();
        };
        let mut wanted = Vec::new();
        for index in indexes {
            self.pages.focus_on(index);
            let pages = self.pages.claim_pages_around(index, self.total);
            wanted.extend(pages.into_iter().map(|page| WantedPage {
                generation: self.generation,
                source: source.clone(),
                page,
            }));
        }
        wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPE: PageShape = PageShape {
        page_size: 4,
        capacity: 2,
    };

    fn grid_with_total(total: usize) -> GridPages<&'static str, usize> {
        let mut grid = GridPages::new(SHAPE);
        grid.restart(Some("albums"));
        grid.set_total(total);
        grid.set_columns(3);
        grid
    }

    #[test]
    fn given_columns_when_rows_are_asked_for_then_the_last_row_is_short() {
        let grid = grid_with_total(7);
        assert_eq!(grid.row_count(), 3);
        assert_eq!(grid.row_indexes(2), Some(6..7));
        assert_eq!(grid.row_indexes(3), None);
        assert_eq!(grid.rows_of([0, 4, 5]), BTreeSet::from([0, 1]));
    }

    #[test]
    fn given_a_row_when_its_pages_are_claimed_then_each_page_is_wanted_once() {
        let mut grid = grid_with_total(20);
        let pages: Vec<usize> = grid
            .claim_pages_for_row(1)
            .into_iter()
            .map(|wanted| wanted.page)
            .collect();
        assert_eq!(pages, [0, 1, 2]);
        assert!(grid.claim_pages_for_row(1).is_empty());
        assert!(grid.is_waiting_for(1, 2));
        grid.page_failed(1, 2);
        assert!(!grid.is_waiting_for(1, 2));
    }

    #[test]
    fn given_no_source_when_pages_are_claimed_then_none_are_wanted() {
        let mut grid: GridPages<&str, usize> = GridPages::new(SHAPE);
        grid.set_total(8);
        assert!(grid.claim_pages_for_row(0).is_empty());
    }

    #[test]
    fn given_a_changed_total_when_relaid_then_only_the_new_page_is_kept_under_a_new_generation() {
        let mut grid = grid_with_total(20);
        grid.insert(0, vec![0, 1, 2, 3]);
        grid.relay(1, vec![4, 5, 6, 7], 9);
        assert!(grid.is_current(2));
        assert_eq!(grid.total(), 9);
        assert_eq!(grid.card(0), None);
        assert_eq!(grid.card(5), Some(&5));
        assert_eq!(grid.first_index_of(1), 4);
        assert_eq!(grid.cards().count(), 4);
    }
}
