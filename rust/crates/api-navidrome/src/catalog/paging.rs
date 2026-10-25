use std::future::Future;

use cadence_api::Result;
use cadence_core::model::Page;

const MAX_COUNTED_PAGES: usize = 2_000;

pub(crate) fn subsonic_page<Item>(items: Vec<Item>, start_index: usize) -> Page<Item> {
    Page {
        items,
        start_index,
        total_count: None,
    }
}

pub(crate) async fn count_by_paging<Fetch, Pending>(page_size: usize, fetch: Fetch) -> Result<usize>
where
    Fetch: Fn(usize) -> Pending,
    Pending: Future<Output = Result<usize>>,
{
    let mut counted = 0;
    for _ in 0..MAX_COUNTED_PAGES {
        let received = fetch(counted).await?;
        counted += received;
        if received < page_size {
            return Ok(counted);
        }
    }
    tracing::debug!(counted, "stopped counting after the page limit");
    Ok(counted)
}
