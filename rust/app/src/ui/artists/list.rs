use std::cell::{Cell, RefCell};

use cadence_core::model::ArtistListQuery;
use slint::{Image, ModelNotify};

use super::super::ArtistCard;
use super::super::cover_lru::CoverLru;
use super::item::ArtistItem;

pub const PAGE_SIZE: usize = 100;
const PREFETCH_ROWS: usize = 40;
const COVER_CAPACITY: usize = 96;

pub enum Request {
    Page {
        generation: u64,
        query: ArtistListQuery,
        start: usize,
    },
    Cover {
        generation: u64,
        index: usize,
        cover_art_id: String,
    },
}

struct ListState {
    generation: u64,
    query: ArtistListQuery,
    items: Vec<ArtistItem>,
    is_fetching: bool,
    is_complete: bool,
    has_failed: bool,
    covers: CoverLru<usize, Image>,
}

impl Default for ListState {
    fn default() -> Self {
        Self {
            generation: 0,
            query: ArtistListQuery::default(),
            items: Vec::new(),
            is_fetching: false,
            is_complete: false,
            has_failed: false,
            covers: CoverLru::new(COVER_CAPACITY),
        }
    }
}

impl ListState {
    fn needs_next_page_near(&self, index: usize) -> bool {
        !self.is_complete
            && !self.is_fetching
            && !self.has_failed
            && index + PREFETCH_ROWS >= self.items.len()
    }

    fn claim_next_page_near(&mut self, index: usize) -> Option<Request> {
        if !self.needs_next_page_near(index) {
            return None;
        }
        self.is_fetching = true;
        Some(Request::Page {
            generation: self.generation,
            query: self.query.clone(),
            start: self.items.len(),
        })
    }

    fn claim_cover(&mut self, index: usize) -> Option<Request> {
        let cover_art_id = self.items.get(index)?.cover_art_id.clone()?;
        self.covers.begin(&index).then_some(Request::Cover {
            generation: self.generation,
            index,
            cover_art_id,
        })
    }

    fn claim_card_requests(&mut self, index: usize) -> Vec<Request> {
        if index >= self.items.len() {
            return Vec::new();
        }
        [self.claim_next_page_near(index), self.claim_cover(index)]
            .into_iter()
            .flatten()
            .collect()
    }

    fn card(&mut self, index: usize) -> Option<ArtistCard> {
        let item = self.items.get(index)?.clone();
        let image = self.covers.get(&index);
        Some(item.into_card(image))
    }

    fn replace_item(&mut self, index: usize, item: ArtistItem) -> bool {
        let Some(shown) = self.items.get_mut(index) else {
            self.items.push(item);
            return false;
        };
        if *shown == item {
            return false;
        }
        if shown.cover_art_id != item.cover_art_id {
            self.covers.forget(&index);
        }
        *shown = item;
        true
    }

    fn drop_rows_after(&mut self, page_end: usize) {
        for index in page_end..self.items.len() {
            self.covers.forget(&index);
        }
        self.items.truncate(page_end);
    }

    fn merge_page(&mut self, start: usize, items: Vec<ArtistItem>) -> PageChange {
        let old_len = self.items.len();
        let page_end = start + items.len();
        let is_short = items.len() < PAGE_SIZE;
        let replaced = items
            .into_iter()
            .enumerate()
            .filter_map(|(offset, item)| {
                let index = start + offset;
                self.replace_item(index, item).then_some(index)
            })
            .collect();
        if is_short {
            self.drop_rows_after(page_end);
        }
        if start == old_len || is_short {
            self.is_fetching = false;
        }
        if page_end >= old_len || is_short {
            self.is_complete = is_short;
        }
        PageChange {
            replaced,
            old_len,
            new_len: self.items.len(),
        }
    }
}

struct PageChange {
    replaced: Vec<usize>,
    old_len: usize,
    new_len: usize,
}

pub struct ArtistList {
    state: RefCell<ListState>,
    columns: Cell<usize>,
    list_notify: ModelNotify,
    grid_notify: ModelNotify,
    fetch: Box<dyn Fn(Request)>,
}

impl ArtistList {
    pub fn new(fetch: impl Fn(Request) + 'static) -> Self {
        Self {
            state: RefCell::default(),
            columns: Cell::new(1),
            list_notify: ModelNotify::default(),
            grid_notify: ModelNotify::default(),
            fetch: Box::new(fetch),
        }
    }

    pub fn len(&self) -> usize {
        self.state.borrow().items.len()
    }

    pub fn is_complete(&self) -> bool {
        self.state.borrow().is_complete
    }

    pub fn columns(&self) -> usize {
        self.columns.get()
    }

    pub fn list_notify(&self) -> &ModelNotify {
        &self.list_notify
    }

    pub fn grid_notify(&self) -> &ModelNotify {
        &self.grid_notify
    }

    pub fn reset(&self, query: ArtistListQuery) {
        let generation = {
            let mut state = self.state.borrow_mut();
            *state = ListState {
                generation: state.generation + 1,
                query: query.clone(),
                is_fetching: true,
                ..ListState::default()
            };
            state.generation
        };
        self.list_notify.reset();
        self.grid_notify.reset();
        (self.fetch)(Request::Page {
            generation,
            query,
            start: 0,
        });
    }

    pub fn set_columns(&self, columns: usize) {
        let columns = columns.max(1);
        if self.columns.replace(columns) != columns {
            self.grid_notify.reset();
        }
    }

    pub fn grid_rows(&self, len: usize) -> usize {
        len.div_ceil(self.columns.get())
    }

    fn notify_appended(&self, old_len: usize, new_len: usize) {
        self.list_notify.row_added(old_len, new_len - old_len);
        let (old_rows, new_rows) = (self.grid_rows(old_len), self.grid_rows(new_len));
        if !old_len.is_multiple_of(self.columns.get()) {
            self.grid_notify.row_changed(old_rows - 1);
        }
        if new_rows > old_rows {
            self.grid_notify.row_added(old_rows, new_rows - old_rows);
        }
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.state.borrow().generation == generation
    }

    fn is_loaded_page(&self, generation: u64, start: usize) -> bool {
        let state = self.state.borrow();
        state.generation == generation && start <= state.items.len()
    }

    pub fn apply_page(&self, generation: u64, start: usize, items: Vec<ArtistItem>) {
        if !self.is_loaded_page(generation, start) {
            return;
        }
        let change = self.state.borrow_mut().merge_page(start, items);
        for index in &change.replaced {
            self.notify_index_changed(*index);
        }
        if change.new_len > change.old_len {
            self.notify_appended(change.old_len, change.new_len);
        }
        if change.new_len < change.old_len {
            self.notify_removed(change.old_len, change.new_len);
        }
    }

    fn notify_removed(&self, old_len: usize, new_len: usize) {
        self.list_notify.row_removed(new_len, old_len - new_len);
        let (old_rows, new_rows) = (self.grid_rows(old_len), self.grid_rows(new_len));
        if new_rows < old_rows {
            self.grid_notify.row_removed(new_rows, old_rows - new_rows);
        }
        if !new_len.is_multiple_of(self.columns.get()) {
            self.grid_notify.row_changed(new_rows - 1);
        }
    }

    pub fn item(&self, artist_id: &str) -> Option<ArtistItem> {
        let state = self.state.borrow();
        state
            .items
            .iter()
            .find(|item| item.id == artist_id)
            .cloned()
    }

    pub fn page_failed(&self, generation: u64) {
        let mut state = self.state.borrow_mut();
        if state.generation == generation {
            state.is_fetching = false;
            state.has_failed = true;
        }
    }

    fn notify_index_changed(&self, index: usize) {
        self.list_notify.row_changed(index);
        self.grid_notify.row_changed(index / self.columns.get());
    }

    pub fn apply_cover(&self, generation: u64, index: usize, image: Option<Image>) {
        {
            let mut state = self.state.borrow_mut();
            if state.generation != generation {
                return;
            }
            match image {
                Some(image) => {
                    state.covers.insert(index, image);
                }
                None => state.covers.retry_later(&index),
            }
        }
        self.notify_index_changed(index);
    }

    pub fn set_favorite(&self, artist_id: &str, is_favorite: bool) {
        let index = {
            let mut state = self.state.borrow_mut();
            let Some((index, item)) = state
                .items
                .iter_mut()
                .enumerate()
                .find(|(_, item)| item.id == artist_id)
            else {
                return;
            };
            item.is_favorite = is_favorite;
            index
        };
        self.notify_index_changed(index);
    }

    pub fn request_card(&self, index: usize) {
        let requests = self.state.borrow_mut().claim_card_requests(index);
        for request in requests {
            (self.fetch)(request);
        }
    }

    pub fn card(&self, index: usize) -> Option<ArtistCard> {
        self.state.borrow_mut().card(index)
    }
}

#[cfg(test)]
pub mod tests {
    use std::rc::Rc;

    use super::*;

    pub type RequestLog = Rc<RefCell<Vec<String>>>;

    fn describe(request: &Request) -> String {
        match request {
            Request::Page {
                generation, start, ..
            } => format!("page {generation} {start}"),
            Request::Cover {
                generation, index, ..
            } => format!("cover {generation} {index}"),
        }
    }

    pub fn list() -> (ArtistList, RequestLog) {
        let log: RequestLog = Rc::default();
        let sink = Rc::clone(&log);
        let list = ArtistList::new(move |request| sink.borrow_mut().push(describe(&request)));
        (list, log)
    }

    pub fn items(start: usize, count: usize) -> Vec<ArtistItem> {
        (start..start + count)
            .map(|index| ArtistItem {
                id: format!("artist-{index}"),
                name: format!("Artist {index}"),
                album_count: Some(2),
                song_count: Some(1),
                is_favorite: false,
                cover_art_id: (index % 2 == 0).then(|| format!("ar-{index}")),
            })
            .collect()
    }

    fn drain(log: &RequestLog) -> Vec<String> {
        log.borrow_mut().drain(..).collect()
    }

    #[test]
    fn pages_until_a_short_page() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        assert_eq!(drain(&log), ["page 1 0"]);

        list.apply_page(1, 0, items(0, PAGE_SIZE));
        assert!(!list.is_complete());
        list.request_card(10);
        assert_eq!(drain(&log), ["cover 1 10"]);

        list.request_card(PAGE_SIZE - 1);
        list.request_card(PAGE_SIZE - 2);
        assert_eq!(
            drain(&log),
            [
                format!("page 1 {PAGE_SIZE}"),
                format!("cover 1 {}", PAGE_SIZE - 2)
            ]
        );
        list.apply_page(1, 0, items(0, PAGE_SIZE));
        list.apply_page(1, PAGE_SIZE, items(PAGE_SIZE, 5));
        assert!(list.is_complete());
        assert_eq!(list.len(), PAGE_SIZE + 5);
        list.request_card(PAGE_SIZE + 4);
        assert_eq!(drain(&log), [format!("cover 1 {}", PAGE_SIZE + 4)]);
    }

    #[test]
    fn stale_generations_are_ignored() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.reset(ArtistListQuery::default());
        assert_eq!(drain(&log), ["page 1 0", "page 2 0"]);
        list.apply_page(1, 0, items(0, 3));
        assert!(!list.is_current(1));
        list.page_failed(1);
        assert_eq!(list.len(), 0);
        list.apply_page(2, 0, items(0, 3));
        list.apply_cover(1, 0, Some(Image::default()));
        assert!(list.card(0).is_some_and(|card| !card.has_image));
    }

    #[test]
    fn failed_page_stops_paging_until_reset() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(1, PAGE_SIZE));
        list.request_card(PAGE_SIZE - 1);
        drain(&log);
        assert!(list.is_current(1));
        list.page_failed(1);
        list.request_card(PAGE_SIZE - 1);
        assert!(drain(&log).is_empty());
    }

    #[test]
    fn covers_are_cached() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, 4));
        drain(&log);
        list.request_card(0);
        list.request_card(0);
        assert_eq!(drain(&log), ["cover 1 0"]);
        list.apply_cover(1, 0, Some(Image::default()));
        assert!(list.card(0).is_some_and(|card| card.has_image));
        assert!(drain(&log).is_empty());
    }

    #[test]
    fn given_a_loaded_page_when_the_refresh_matches_then_covers_and_paging_stay() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, PAGE_SIZE));
        list.apply_cover(1, 0, Some(Image::default()));
        list.request_card(PAGE_SIZE - 1);
        drain(&log);

        list.apply_page(1, 0, items(0, PAGE_SIZE));

        assert!(list.card(0).is_some_and(|card| card.has_image));
        list.request_card(PAGE_SIZE - 2);
        assert_eq!(drain(&log), [format!("cover 1 {}", PAGE_SIZE - 2)]);
    }

    #[test]
    fn given_a_loaded_page_when_the_refresh_differs_then_rows_change_in_place() {
        let (list, _log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, 3));
        list.apply_cover(1, 0, Some(Image::default()));
        list.apply_cover(1, 2, Some(Image::default()));
        let mut refreshed = items(0, 3);
        refreshed[2].name = "Renamed".to_owned();
        refreshed[0].cover_art_id = Some("ar-new".to_owned());

        list.apply_page(1, 0, refreshed);

        assert_eq!(list.len(), 3);
        assert!(list.card(2).is_some_and(|card| card.name == "Renamed"));
        assert!(list.card(2).is_some_and(|card| card.has_image));
        assert!(list.card(0).is_some_and(|card| !card.has_image));
    }

    #[test]
    fn given_a_shorter_refresh_when_applied_then_the_tail_goes_and_the_list_completes() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, PAGE_SIZE));
        list.apply_page(1, PAGE_SIZE, items(PAGE_SIZE, PAGE_SIZE));
        list.request_card(2 * PAGE_SIZE - 1);
        drain(&log);

        list.apply_page(1, 0, items(0, 5));

        assert_eq!(list.len(), 5);
        assert!(list.is_complete());
        list.apply_page(1, 2 * PAGE_SIZE, items(2 * PAGE_SIZE, 5));
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn given_a_short_cached_page_when_the_refresh_is_full_then_paging_resumes() {
        let (list, log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, 5));
        assert!(list.is_complete());
        drain(&log);

        list.apply_page(1, 0, items(0, PAGE_SIZE));

        assert_eq!(list.len(), PAGE_SIZE);
        assert!(!list.is_complete());
        list.request_card(PAGE_SIZE - 1);
        assert_eq!(drain(&log)[0], format!("page 1 {PAGE_SIZE}"));
    }

    #[test]
    fn favorites_update_loaded_items() {
        let (list, _log) = list();
        list.reset(ArtistListQuery::default());
        list.apply_page(1, 0, items(0, 3));
        list.set_favorite("artist-1", true);
        assert!(list.card(1).is_some_and(|card| card.is_favorite));
    }
}
