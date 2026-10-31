use std::cell::RefCell;
use std::collections::BTreeSet;

use cadence_core::model::{AlbumListQuery, Page};
use slint::{Image, Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::super::cover_lru::CoverLru;
use super::super::grid_pages::{GridPages, WantedPage};
use super::super::page_cache::{PageShape, changed_total};
use super::super::rows::notify_row_count;
use super::super::{AlbumGridCard, AlbumGridRow};
use super::card::CardData;

pub const PAGE_SIZE: usize = 60;
const COVER_CAPACITY: usize = 96;
const CACHED_PAGES: usize = 16;
const GRID_PAGES: PageShape = PageShape {
    page_size: PAGE_SIZE,
    capacity: CACHED_PAGES,
};

#[derive(Debug, Clone, PartialEq)]
pub enum GridRequest {
    Count {
        generation: u64,
        query: AlbumListQuery,
    },
    Page {
        generation: u64,
        query: AlbumListQuery,
        page: usize,
    },
    Cover {
        cover_art_id: String,
    },
}

struct GridState {
    grid: GridPages<AlbumListQuery, CardData>,
    covers: CoverLru<String, Image>,
}

impl GridState {
    fn indexes_with_cover(&self, cover_art_id: &str) -> Vec<usize> {
        self.grid
            .cards()
            .filter(|(_, card)| card.cover_art_id.as_deref() == Some(cover_art_id))
            .map(|(index, _)| index)
            .collect()
    }

    fn claim_row(&mut self, row: usize) -> Vec<GridRequest> {
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
        let cover_art_ids: Vec<String> = indexes
            .filter_map(|index| self.grid.card(index)?.cover_art_id.clone())
            .collect();
        cover_art_ids
            .into_iter()
            .filter(|cover_art_id| self.covers.begin(cover_art_id.as_str()))
            .map(|cover_art_id| GridRequest::Cover { cover_art_id })
            .collect()
    }

    fn grid_card(&mut self, index: usize) -> AlbumGridCard {
        let Some(data) = self.grid.card(index).cloned() else {
            return AlbumGridCard::default();
        };
        let cover = data
            .cover_art_id
            .as_deref()
            .and_then(|cover_art_id| self.covers.get(cover_art_id));
        AlbumGridCard {
            id: data.id,
            name: data.name,
            artists: data.artists,
            artist_id: data.artist_id,
            year: data.year,
            has_cover: cover.is_some(),
            cover: cover.unwrap_or_default(),
            is_loaded: true,
        }
    }

    fn row_cards(&mut self, row: usize) -> Option<Vec<AlbumGridCard>> {
        let indexes = self.grid.row_indexes(row)?;
        Some(indexes.map(|index| self.grid_card(index)).collect())
    }
}

fn page_request(wanted: WantedPage<AlbumListQuery>) -> GridRequest {
    GridRequest::Page {
        generation: wanted.generation,
        query: wanted.source,
        page: wanted.page,
    }
}

pub struct AlbumGridModel {
    state: RefCell<GridState>,
    notify: ModelNotify,
    fetch: Box<dyn Fn(GridRequest)>,
}

impl AlbumGridModel {
    pub fn new(fetch: impl Fn(GridRequest) + 'static) -> Self {
        Self {
            state: RefCell::new(GridState {
                grid: GridPages::new(GRID_PAGES),
                covers: CoverLru::new(COVER_CAPACITY),
            }),
            notify: ModelNotify::default(),
            fetch: Box::new(fetch),
        }
    }

    pub fn reset(&self, query: AlbumListQuery) {
        let generation = self.state.borrow_mut().grid.restart(Some(query.clone()));
        self.notify.reset();
        (self.fetch)(GridRequest::Count { generation, query });
    }

    pub fn set_columns(&self, columns: usize) {
        {
            let mut state = self.state.borrow_mut();
            if state.grid.columns() == columns.max(1) {
                return;
            }
            state.grid.set_columns(columns);
        }
        self.notify.reset();
    }

    pub fn clear_covers(&self) {
        self.state.borrow_mut().covers.clear();
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.state.borrow().grid.is_current(generation)
    }

    pub fn reported_total_change(
        &self,
        generation: u64,
        reported_total: Option<usize>,
    ) -> Option<usize> {
        let state = self.state.borrow();
        if !state.grid.is_current(generation) {
            return None;
        }
        changed_total(state.grid.total(), reported_total)
    }

    pub fn apply_count(&self, generation: u64, total: usize) {
        let (before, after) = {
            let mut state = self.state.borrow_mut();
            if !state.grid.is_current(generation) {
                return;
            }
            let before = state.grid.row_count();
            state.grid.set_total(total);
            (before, state.grid.row_count())
        };
        notify_row_count(&self.notify, before, after);
        if let Some(last_kept_row) = before.min(after).checked_sub(1) {
            self.notify.row_changed(last_kept_row);
        }
    }

    fn notify_rows_changed(&self, rows: BTreeSet<usize>) {
        for row in rows {
            self.notify.row_changed(row);
        }
    }

    pub fn apply_page(&self, generation: u64, page: usize, cards: Vec<CardData>) {
        let rows = {
            let mut state = self.state.borrow_mut();
            if !state.grid.is_current(generation) {
                return;
            }
            let start = state.grid.first_index_of(page);
            let end = (start + cards.len()).min(state.grid.total());
            state.grid.insert(page, cards);
            state.grid.rows_of(start..end)
        };
        self.notify_rows_changed(rows);
    }

    pub fn apply_listed_page(&self, generation: u64, page: usize, listed: Page<CardData>) {
        let Some(total) = self.reported_total_change(generation, listed.total_count) else {
            self.apply_page(generation, page, listed.items);
            return;
        };
        self.relay_cards(page, total, listed.items);
    }

    fn relay_cards(&self, page: usize, total: usize, cards: Vec<CardData>) {
        tracing::debug!(
            total,
            "the server album list changed size, reloading the cached pages"
        );
        self.state.borrow_mut().grid.relay(page, cards, total);
        self.notify.reset();
    }

    pub fn page_failed(&self, generation: u64, page: usize) {
        self.state.borrow_mut().grid.page_failed(generation, page);
    }

    pub fn apply_cover(&self, cover_art_id: &str, image: Option<Image>) {
        let rows = {
            let mut state = self.state.borrow_mut();
            let Some(image) = image else {
                state.covers.fail(cover_art_id);
                return;
            };
            state.covers.insert(cover_art_id.to_owned(), image);
            let indexes = state.indexes_with_cover(cover_art_id);
            state.grid.rows_of(indexes)
        };
        self.notify_rows_changed(rows);
    }

    fn request_row(&self, row: usize) {
        let requests = self.state.borrow_mut().claim_row(row);
        for request in requests {
            (self.fetch)(request);
        }
    }
}

impl Model for AlbumGridModel {
    type Data = AlbumGridRow;

    fn row_count(&self) -> usize {
        self.state.borrow().grid.row_count()
    }

    fn row_data(&self, row: usize) -> Option<AlbumGridRow> {
        self.request_row(row);
        let cards = self.state.borrow_mut().row_cards(row)?;
        Some(AlbumGridRow {
            cards: ModelRc::new(VecModel::from(cards)),
        })
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

    use super::super::card::tests::album;
    use super::*;

    type RequestLog = Rc<RefCell<Vec<GridRequest>>>;

    fn recording() -> (RequestLog, AlbumGridModel) {
        let log: RequestLog = Rc::default();
        let sink = Rc::clone(&log);
        let model = AlbumGridModel::new(move |request| sink.borrow_mut().push(request));
        (log, model)
    }

    fn drain(log: &RequestLog) -> Vec<GridRequest> {
        log.borrow_mut().drain(..).collect()
    }

    fn counted_model_with_four_columns() -> (RequestLog, AlbumGridModel) {
        let (log, model) = recording();
        model.set_columns(4);
        model.reset(AlbumListQuery::default());
        assert!(matches!(
            drain(&log).as_slice(),
            [GridRequest::Count { generation: 1, .. }]
        ));
        model.apply_count(1, 130);
        assert_eq!(model.row_count(), 33);
        (log, model)
    }

    fn assert_missing_page_is_requested_once(log: &RequestLog, model: &AlbumGridModel) {
        let row = model.row_data(15).expect("row");
        assert_eq!(row.cards.row_count(), 4);
        assert!(row.cards.iter().all(|card| !card.is_loaded));
        model.row_data(16);
        assert!(matches!(
            drain(log).as_slice(),
            [
                GridRequest::Page {
                    generation: 1,
                    page: 1,
                    ..
                },
                GridRequest::Page {
                    generation: 1,
                    page: 2,
                    ..
                }
            ]
        ));
    }

    fn assert_loaded_page_requests_covers_once(log: &RequestLog, model: &AlbumGridModel) {
        let cards = (60..120)
            .map(|index| CardData::from_album(&album(index)))
            .collect();
        model.apply_page(1, 1, cards);
        let row = model.row_data(15).expect("row");
        assert_eq!(
            row.cards.row_data(0).map(|card| card.id),
            Some("album-60".into())
        );
        assert!(
            row.cards
                .iter()
                .all(|card| card.is_loaded && !card.has_cover)
        );
        let covers = drain(log)
            .iter()
            .filter(|request| matches!(request, GridRequest::Cover { .. }))
            .count();
        assert_eq!(covers, 4);
        model.row_data(15);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn rows_regroup_by_columns_and_request_pages_and_covers_once() {
        let (log, model) = counted_model_with_four_columns();
        assert_missing_page_is_requested_once(&log, &model);
        assert_loaded_page_requests_covers_once(&log, &model);

        model.apply_cover("al-61", Some(Image::default()));
        let row = model.row_data(15).expect("row");
        assert_eq!(row.cards.row_data(1).map(|card| card.has_cover), Some(true));

        model.set_columns(10);
        assert_eq!(model.row_count(), 13);
        assert_eq!(model.row_data(13), None);

        model.reset(AlbumListQuery::default());
        model.apply_page(1, 0, vec![CardData::default()]);
        assert!(!model.is_current(1));
        model.apply_count(1, 5);
        assert_eq!(model.row_count(), 0);
    }

    fn requested_covers(log: &RequestLog) -> usize {
        drain(log)
            .iter()
            .filter(|request| matches!(request, GridRequest::Cover { .. }))
            .count()
    }

    #[test]
    fn given_covers_dropped_by_leaving_the_view_when_it_loads_again_then_they_are_requested() {
        let (log, model) = counted_model_with_four_columns();
        model.apply_page(1, 0, album_page(0));
        model.row_data(0);
        let dropped = requested_covers(&log);
        assert_eq!(dropped, 4);
        for index in 0..dropped {
            let card = CardData::from_album(&album(index));
            model.apply_cover(card.cover_art_id.as_deref().expect("a cover id"), None);
        }

        model.row_data(0);
        assert_eq!(requested_covers(&log), 0);

        model.clear_covers();
        model.row_data(0);
        assert_eq!(requested_covers(&log), dropped);
    }

    fn requested_pages(log: &RequestLog) -> Vec<usize> {
        drain(log)
            .into_iter()
            .filter_map(|request| match request {
                GridRequest::Page { page, .. } => Some(page),
                _ => None,
            })
            .collect()
    }

    fn album_page(page: usize) -> Vec<CardData> {
        (page * PAGE_SIZE..(page + 1) * PAGE_SIZE)
            .map(|index| CardData::from_album(&album(index)))
            .collect()
    }

    #[test]
    fn far_pages_are_evicted_and_fetched_again() {
        let (log, model) = recording();
        model.set_columns(PAGE_SIZE);
        model.reset(AlbumListQuery::default());
        model.apply_count(1, PAGE_SIZE * 40);
        drain(&log);
        for row in 0..40 {
            model.row_data(row);
            for page in requested_pages(&log) {
                model.apply_page(1, page, album_page(page));
            }
        }
        let first = model.row_data(0).expect("row");
        assert!(first.cards.iter().all(|card| !card.is_loaded));
        assert_eq!(requested_pages(&log), [0]);
        let kept = model.row_data(39 - CACHED_PAGES + 1).expect("row");
        assert!(kept.cards.iter().all(|card| card.is_loaded));
        let evicted = model.row_data(39 - CACHED_PAGES).expect("row");
        assert!(evicted.cards.iter().all(|card| !card.is_loaded));
    }

    fn listed_page(page: usize, total_count: Option<usize>) -> Page<CardData> {
        Page {
            items: album_page(page),
            start_index: page * PAGE_SIZE,
            total_count,
        }
    }

    #[test]
    fn a_changed_server_total_reloads_the_cached_albums() {
        let (log, model) = recording();
        model.set_columns(PAGE_SIZE);
        model.reset(AlbumListQuery::default());
        model.apply_count(1, PAGE_SIZE * 10);
        drain(&log);
        model.row_data(0);
        assert_eq!(requested_pages(&log), [0, 1]);
        let same = Some(PAGE_SIZE * 10);
        assert_eq!(model.reported_total_change(1, same), None);
        model.apply_listed_page(1, 0, listed_page(0, same));
        let shrunk = Some(PAGE_SIZE * 10 - 1);
        assert_eq!(model.reported_total_change(1, shrunk), shrunk);
        model.apply_listed_page(1, 1, listed_page(1, shrunk));
        assert_eq!(model.reported_total_change(1, shrunk), None);
        assert_eq!(model.row_count(), 10);
        let first = model.row_data(0).expect("row");
        assert!(first.cards.iter().all(|card| !card.is_loaded));
        assert!(matches!(
            drain(&log).as_slice(),
            [GridRequest::Page {
                generation: 2,
                page: 0,
                ..
            }]
        ));
    }

    #[test]
    fn a_failing_album_page_waits_before_it_is_asked_for_again() {
        let (log, model) = recording();
        model.set_columns(PAGE_SIZE);
        model.reset(AlbumListQuery::default());
        model.apply_count(1, PAGE_SIZE * 10);
        drain(&log);
        model.row_data(4);
        assert_eq!(requested_pages(&log), [4, 5]);
        model.page_failed(1, 4);
        model.row_data(8);
        model.row_data(4);
        assert!(!requested_pages(&log).contains(&4));
    }
}
