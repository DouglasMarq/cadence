use cadence_core::model::Paging;

use crate::ui::page_cache::{PageShape, page_paging};

pub const PAGE_SIZE: usize = 100;
pub const MAX_CACHED_PAGES: usize = 8;
pub const PLAYLIST_PAGES: PageShape = PageShape {
    page_size: PAGE_SIZE,
    capacity: MAX_CACHED_PAGES,
};

pub fn paging_for_page(page: usize) -> Paging {
    page_paging(page, PAGE_SIZE)
}
