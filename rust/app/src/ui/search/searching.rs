use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::{Paging, SearchResults};

use super::fetch::{ServerCall, fetch};
use super::query::{SEARCH_LIMIT, album_query, artist_query, song_query};
use super::results::{Results, with_results};
use super::searches::{More, MoreRequest};
use crate::ui::{AppWindow, Context, SearchKind, show_status};

pub struct SearchRequest {
    pub query: Option<String>,
    pub is_forced: bool,
}

#[derive(Clone, Copy)]
struct MorePage {
    generation: u64,
    kind: SearchKind,
    paging: Paging,
}

pub fn search(window: &AppWindow, context: &Context, request: SearchRequest) {
    let SearchRequest { query, is_forced } = request;
    let started = with_results(window, |results| {
        results.begin_search(window, query.clone(), is_forced)
    })
    .flatten();
    let (Some(generation), Some(query)) = (started, query) else {
        return;
    };
    let sent = fetch(
        window,
        context,
        ServerCall::new(
            move |server| async move { server.search(&query, SEARCH_LIMIT).await },
            move |window, results, result| {
                results.finish_search(window, generation, result);
            },
        ),
    );
    if sent.is_err() {
        with_results(window, |results| results.abandon_search(window, generation));
    }
}

pub fn show_more(window: &AppWindow, context: &Context, kind: SearchKind) {
    let requested = with_results(window, |results| {
        let request = results.searches.borrow_mut().more(kind);
        results.show_flags(window);
        request
    })
    .flatten();
    let Some(MoreRequest {
        generation,
        query,
        paging,
    }) = requested
    else {
        return;
    };
    let page = MorePage {
        generation,
        kind,
        paging,
    };
    let sent = fetch(
        window,
        context,
        ServerCall::new(
            move |server| fetch_more(server, query, page),
            move |window, results, result| {
                results.apply_more(window, page, result);
            },
        ),
    );
    if sent.is_err() {
        with_results(window, |results| results.abandon_more(window, page));
    }
}

async fn fetch_more(
    server: Arc<dyn MusicServer>,
    query: String,
    page: MorePage,
) -> cadence_api::Result<More> {
    let paging = page.paging;
    match page.kind {
        SearchKind::Songs => server
            .song_list(&song_query(&query), paging)
            .await
            .map(|page| More::Songs(page.items)),
        SearchKind::Albums => server
            .album_list(&album_query(&query), paging)
            .await
            .map(|page| More::Albums(page.items)),
        SearchKind::Artists => server
            .artist_list(&artist_query(&query), paging)
            .await
            .map(|page| More::Artists(page.items)),
    }
}

impl Results {
    fn finish_search(
        &self,
        window: &AppWindow,
        generation: u64,
        result: cadence_api::Result<SearchResults>,
    ) {
        let found = result
            .inspect_err(|error| show_status(window, &tr!("Search failed: {}", error)))
            .ok();
        let is_current = self.searches.borrow().is_current(generation);
        self.searches
            .borrow_mut()
            .finish(generation, found, SEARCH_LIMIT);
        if is_current {
            self.show_all(window);
        }
    }

    fn abandon_search(&self, window: &AppWindow, generation: u64) {
        self.searches
            .borrow_mut()
            .finish(generation, None, SEARCH_LIMIT);
        self.show_flags(window);
    }

    fn apply_more(&self, window: &AppWindow, page: MorePage, result: cadence_api::Result<More>) {
        let MorePage {
            generation,
            kind,
            paging,
        } = page;
        let placement = match result {
            Ok(more) => {
                let mut searches = self.searches.borrow_mut();
                let placement = searches.placement_of_more(generation, kind, paging);
                searches.apply_more(generation, paging, more);
                placement
            }
            Err(error) => {
                self.searches.borrow_mut().more_failed(generation, kind);
                show_status(window, &tr!("Could not load more results: {}", error));
                None
            }
        };
        match placement {
            Some(placement) => self.show_more(window, kind, &placement),
            None => self.show_flags(window),
        }
    }

    fn abandon_more(&self, window: &AppWindow, page: MorePage) {
        self.searches
            .borrow_mut()
            .more_failed(page.generation, page.kind);
        self.show_flags(window);
    }
}
