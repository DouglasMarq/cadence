use std::collections::{HashMap, HashSet};
use std::time::Instant;

use cadence_core::model::Paging;

use super::page_backoff::PageBackoff;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ScrollDirection {
    #[default]
    Forward,
    Backward,
}

pub fn page_paging(page: usize, page_size: usize) -> Paging {
    Paging {
        start_index: page.saturating_mul(page_size),
        limit: page_size,
    }
}

pub fn changed_total(known: usize, reported: Option<usize>) -> Option<usize> {
    reported.filter(|total| *total != known)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageShape {
    pub page_size: usize,
    pub capacity: usize,
}

#[derive(Debug)]
pub struct PageCache<Row> {
    shape: PageShape,
    pages: HashMap<usize, Vec<Row>>,
    requested: HashSet<usize>,
    backoff: PageBackoff,
    anchor: usize,
    direction: ScrollDirection,
}

impl<Row> PageCache<Row> {
    pub fn new(shape: PageShape) -> Self {
        Self {
            shape: PageShape {
                page_size: shape.page_size.max(1),
                capacity: shape.capacity.max(1),
            },
            pages: HashMap::new(),
            requested: HashSet::new(),
            backoff: PageBackoff::default(),
            anchor: 0,
            direction: ScrollDirection::default(),
        }
    }

    pub fn page_of(&self, index: usize) -> usize {
        index / self.shape.page_size
    }

    pub fn first_index_of(&self, page: usize) -> usize {
        page.saturating_mul(self.shape.page_size)
    }

    pub fn get(&self, index: usize) -> Option<&Row> {
        self.pages
            .get(&self.page_of(index))?
            .get(index % self.shape.page_size)
    }

    pub fn rows(&self) -> impl Iterator<Item = (usize, &Row)> {
        let page_size = self.shape.page_size;
        self.pages.iter().flat_map(move |(page, rows)| {
            rows.iter()
                .enumerate()
                .map(move |(position, row)| (global_index(*page, page_size, position), row))
        })
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = (usize, &mut Row)> {
        let page_size = self.shape.page_size;
        self.pages.iter_mut().flat_map(move |(page, rows)| {
            rows.iter_mut()
                .enumerate()
                .map(move |(position, row)| (global_index(*page, page_size, position), row))
        })
    }

    pub fn mark_requested(&mut self, page: usize) {
        self.requested.insert(page);
    }

    pub fn is_requested(&self, page: usize) -> bool {
        self.requested.contains(&page)
    }

    pub fn has_page(&self, page: usize) -> bool {
        self.pages.contains_key(&page)
    }

    pub fn claim_page(&mut self, page: usize) -> bool {
        self.claim_page_at(page, Instant::now())
    }

    pub fn claim_page_at(&mut self, page: usize, now: Instant) -> bool {
        self.backoff.allows(page, now) && self.requested.insert(page)
    }

    pub fn request_failed(&mut self, page: usize) {
        self.request_failed_at(page, Instant::now());
    }

    pub fn request_failed_at(&mut self, page: usize, now: Instant) {
        if !self.requested.remove(&page) {
            return;
        }
        self.backoff.record_failure(page, now);
        tracing::debug!(
            page,
            delay = ?self.backoff.delay_of(page),
            "a list page failed, waiting before asking again"
        );
    }

    pub fn invalidate(&mut self) {
        self.pages.clear();
        self.requested.clear();
        self.backoff.clear();
    }

    pub fn focus_on(&mut self, index: usize) {
        let page = self.page_of(index);
        self.direction = match page.cmp(&self.anchor) {
            std::cmp::Ordering::Greater => ScrollDirection::Forward,
            std::cmp::Ordering::Less => ScrollDirection::Backward,
            std::cmp::Ordering::Equal => self.direction,
        };
        self.anchor = page;
    }

    pub fn claim_pages_around(&mut self, index: usize, total: usize) -> Vec<usize> {
        self.claim_pages_around_at(index, total, Instant::now())
    }

    pub fn claim_pages_around_at(
        &mut self,
        index: usize,
        total: usize,
        now: Instant,
    ) -> Vec<usize> {
        let page = self.page_of(index);
        let neighbour = match self.direction {
            ScrollDirection::Forward => page.checked_add(1),
            ScrollDirection::Backward => page.checked_sub(1),
        };
        let page_size = self.shape.page_size;
        [Some(page), neighbour]
            .into_iter()
            .flatten()
            .filter(|candidate| {
                candidate
                    .checked_mul(page_size)
                    .is_some_and(|start| start < total)
            })
            .filter(|candidate| self.backoff.allows(*candidate, now))
            .filter(|candidate| self.requested.insert(*candidate))
            .collect()
    }

    pub fn insert(&mut self, page: usize, rows: Vec<Row>) {
        self.requested.insert(page);
        self.backoff.succeeded(page);
        self.pages.insert(page, rows);
        while self.pages.len() > self.shape.capacity {
            let Some(farthest) = self.farthest_page() else {
                return;
            };
            self.pages.remove(&farthest);
            self.requested.remove(&farthest);
        }
    }

    fn farthest_page(&self) -> Option<usize> {
        let anchor = self.anchor;
        self.pages
            .keys()
            .copied()
            .max_by_key(|page| (page.abs_diff(anchor), *page))
    }
}

fn global_index(page: usize, page_size: usize, position: usize) -> usize {
    page.saturating_mul(page_size).saturating_add(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPE: PageShape = PageShape {
        page_size: 10,
        capacity: 3,
    };

    fn rows_for(cache: &PageCache<usize>, page: usize) -> Vec<usize> {
        let start = cache.first_index_of(page);
        (start..start + cache.shape.page_size).collect()
    }

    fn fetch(cache: &mut PageCache<usize>, index: usize, total: usize) -> Vec<usize> {
        cache.focus_on(index);
        cache.claim_pages_around(index, total)
    }

    const BACKOFF_TOTAL: usize = 100;

    fn fetch_at(cache: &mut PageCache<usize>, index: usize, now: Instant) -> Vec<usize> {
        cache.focus_on(index);
        cache.claim_pages_around_at(index, BACKOFF_TOTAL, now)
    }

    fn load(cache: &mut PageCache<usize>, index: usize, total: usize) -> Vec<usize> {
        let pages = fetch(cache, index, total);
        for page in &pages {
            let rows = rows_for(cache, *page);
            cache.insert(*page, rows);
        }
        pages
    }

    fn loaded(cache: &PageCache<usize>) -> Vec<usize> {
        let mut pages: Vec<usize> = cache.pages.keys().copied().collect();
        pages.sort_unstable();
        pages
    }

    #[test]
    fn requests_the_page_and_prefetches_the_next_one_once() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        assert_eq!(fetch(&mut cache, 5, 100), [0, 1]);
        assert!(fetch(&mut cache, 7, 100).is_empty());
        assert_eq!(fetch(&mut cache, 15, 100), [2]);
        assert_eq!(fetch(&mut cache, 95, 100), [9]);
        assert_eq!(fetch(&mut cache, 55, 100), [5, 4]);
        assert_eq!(cache.get(5), None);
    }

    #[test]
    fn evicts_the_page_farthest_from_the_last_request() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        load(&mut cache, 0, 1000);
        load(&mut cache, 25, 1000);
        assert_eq!(loaded(&cache), [1, 2, 3]);
        assert_eq!(cache.get(25), Some(&25));
        assert_eq!(cache.get(5), None);
        assert!(!cache.requested.contains(&0));

        load(&mut cache, 85, 1000);
        assert_eq!(loaded(&cache), [3, 8, 9]);
        load(&mut cache, 45, 1000);
        assert_eq!(loaded(&cache), [3, 4, 8]);
        assert_eq!(cache.pages.len(), SHAPE.capacity);
    }

    #[test]
    fn evicted_pages_are_fetched_again_when_scrolled_back() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        load(&mut cache, 0, 1000);
        load(&mut cache, 50, 1000);
        load(&mut cache, 70, 1000);
        assert_eq!(loaded(&cache), [6, 7, 8]);
        assert_eq!(cache.get(5), None);
        assert_eq!(load(&mut cache, 5, 1000), [0]);
        assert_eq!(cache.get(5), Some(&5));
        assert_eq!(loaded(&cache), [0, 6, 7]);
    }

    #[test]
    fn late_pages_far_from_the_anchor_are_dropped_and_requestable() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        let early = fetch(&mut cache, 0, 1000);
        load(&mut cache, 50, 1000);
        load(&mut cache, 60, 1000);
        for page in early {
            let rows = rows_for(&cache, page);
            cache.insert(page, rows);
        }
        assert_eq!(loaded(&cache), [5, 6, 7]);
        assert!(!cache.requested.contains(&0));
        assert!(cache.is_requested(7));
        cache.request_failed(7);
        assert!(!cache.is_requested(7));
        cache.mark_requested(7);
        assert!(cache.is_requested(7));
    }

    #[test]
    fn a_failing_page_is_not_requested_again_until_its_backoff_passes() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        let failed_at = Instant::now();
        assert_eq!(fetch_at(&mut cache, 5, failed_at), [0, 1]);
        cache.request_failed_at(0, failed_at);
        assert!(!cache.is_requested(0));
        cache.insert(1, rows_for(&cache, 1));
        for _ in 0..5 {
            fetch_at(&mut cache, 95, failed_at);
            assert!(!fetch_at(&mut cache, 5, failed_at).contains(&0));
        }
        let first_retry = failed_at + std::time::Duration::from_secs(2);
        assert_eq!(fetch_at(&mut cache, 5, first_retry), [0]);
        cache.request_failed_at(0, first_retry);
        let too_soon = first_retry + std::time::Duration::from_secs(3);
        assert!(fetch_at(&mut cache, 5, too_soon).is_empty());
        let second_retry = first_retry + std::time::Duration::from_secs(4);
        assert_eq!(fetch_at(&mut cache, 5, second_retry), [0]);
        cache.insert(0, rows_for(&cache, 0));
        assert_eq!(cache.get(5), Some(&5));
    }

    #[test]
    fn given_a_failed_page_when_it_is_claimed_alone_then_it_waits_for_its_backoff() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        let now = Instant::now();
        assert!(cache.claim_page_at(3, now));
        assert!(!cache.claim_page_at(3, now));
        cache.request_failed_at(3, now);
        assert!(!cache.claim_page_at(3, now));
        assert!(cache.claim_page_at(3, now + std::time::Duration::from_secs(2)));
        assert!(!cache.has_page(3));
        cache.insert(3, rows_for(&cache, 3));
        assert!(cache.has_page(3));
    }

    #[test]
    fn invalidating_drops_every_page_and_request() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        load(&mut cache, 15, 1000);
        assert!(fetch(&mut cache, 45, 1000).contains(&4));
        cache.invalidate();
        assert_eq!(cache.get(15), None);
        assert!(loaded(&cache).is_empty());
        assert_eq!(fetch(&mut cache, 45, 1000), [4, 5]);
    }

    #[test]
    fn given_a_page_number_when_it_becomes_paging_then_it_starts_at_the_first_row_of_the_page() {
        let paging = page_paging(3, 25);
        assert_eq!((paging.start_index, paging.limit), (75, 25));
    }

    #[test]
    fn rows_report_their_global_index() {
        let mut cache = PageCache::<usize>::new(SHAPE);
        load(&mut cache, 30, 1000);
        let mut indexes: Vec<usize> = cache.rows().map(|(index, _)| index).collect();
        indexes.sort_unstable();
        assert_eq!(indexes, (30..50).collect::<Vec<_>>());
        for (index, row) in cache.rows_mut() {
            *row = index * 2;
        }
        assert_eq!(cache.get(41), Some(&82));
    }
}
