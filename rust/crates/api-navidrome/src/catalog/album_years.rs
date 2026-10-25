use cadence_api::{ApiError, Result};
use cadence_api_subsonic::Params;
use cadence_core::model::{Album, AlbumListQuery, Page, Paging, SortOrder};

use crate::NavidromeServer;
use crate::catalog::SUBSONIC_MAX_ITEMS;
use crate::catalog::paging::{count_by_paging, subsonic_page};

const BY_YEAR_LIST_TYPE: &str = "byYear";
const EARLIEST_YEAR: i32 = 1;
const LATEST_YEAR: i32 = 9999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct YearRange {
    first: i32,
    last: i32,
}

impl YearRange {
    pub(crate) fn of(query: &AlbumListQuery) -> Option<Self> {
        match (query.year_from, query.year_to) {
            (None, None) => None,
            (Some(from), Some(to)) if from == to => None,
            (from, to) => Some(Self {
                first: from.unwrap_or(EARLIEST_YEAR),
                last: to.unwrap_or(LATEST_YEAR),
            }),
        }
    }

    fn bounds_in(self, order: SortOrder) -> (i32, i32) {
        match order {
            SortOrder::Asc => (self.first, self.last),
            SortOrder::Desc => (self.last, self.first),
        }
    }
}

fn combines_other_filters(query: &AlbumListQuery) -> bool {
    !query.artist_ids.is_empty()
        || !query.genre_ids.is_empty()
        || query.is_favorite.is_some()
        || query
            .search
            .as_ref()
            .is_some_and(|search| !search.is_empty())
}

fn by_year_params(query: &AlbumListQuery, range: YearRange, window: Paging) -> Params {
    let (from_year, to_year) = range.bounds_in(query.order);
    Params::new()
        .add("type", BY_YEAR_LIST_TYPE)
        .add("fromYear", from_year)
        .add("toYear", to_year)
        .add("size", window.limit)
        .add("offset", window.start_index)
        .add_optional("musicFolderId", query.music_folder_id.as_deref())
}

fn page_size(limit: usize) -> usize {
    match limit {
        0 => SUBSONIC_MAX_ITEMS,
        limit => limit.min(SUBSONIC_MAX_ITEMS),
    }
}

impl NavidromeServer {
    pub(crate) async fn album_page_in_year_range(
        &self,
        query: &AlbumListQuery,
        range: YearRange,
        paging: Paging,
    ) -> Result<Page<Album>> {
        let window = Paging {
            start_index: paging.start_index,
            limit: page_size(paging.limit),
        };
        let params = by_year_params(query, range, window);
        let albums = self.albums_by_year(params, query).await?;
        Ok(subsonic_page(albums, paging.start_index))
    }

    pub(crate) async fn count_albums_in_year_range(
        &self,
        query: &AlbumListQuery,
        range: YearRange,
    ) -> Result<usize> {
        count_by_paging(SUBSONIC_MAX_ITEMS, |start_index| async move {
            let window = Paging {
                start_index,
                limit: SUBSONIC_MAX_ITEMS,
            };
            let params = by_year_params(query, range, window);
            Ok(self.albums_by_year(params, query).await?.len())
        })
        .await
    }

    async fn albums_by_year(&self, params: Params, query: &AlbumListQuery) -> Result<Vec<Album>> {
        if combines_other_filters(query) {
            return Err(ApiError::Unsupported(
                "combining an album year range with other filters",
            ));
        }
        self.subsonic_server.fetch_album_list(params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(year_from: Option<i32>, year_to: Option<i32>, order: SortOrder) -> AlbumListQuery {
        AlbumListQuery {
            year_from,
            year_to,
            order,
            ..AlbumListQuery::default()
        }
    }

    #[test]
    fn a_single_year_is_not_a_range() {
        assert_eq!(
            YearRange::of(&query(Some(2021), Some(2021), SortOrder::Asc)),
            None
        );
        assert_eq!(YearRange::of(&query(None, None, SortOrder::Asc)), None);
    }

    #[test]
    fn open_bounds_reach_the_calendar_limits() {
        let range = YearRange::of(&query(None, Some(1990), SortOrder::Asc));
        assert_eq!(
            range.map(|range| range.bounds_in(SortOrder::Asc)),
            Some((1, 1990))
        );
    }

    #[test]
    fn descending_order_swaps_the_bounds() {
        let descending = query(Some(2020), Some(2022), SortOrder::Desc);
        let range = YearRange::of(&descending).expect("range");
        let params = format!(
            "{:?}",
            by_year_params(
                &descending,
                range,
                Paging {
                    start_index: 10,
                    limit: 5,
                },
            )
        );
        assert!(params.contains("(\"fromYear\", \"2022\")"));
        assert!(params.contains("(\"toYear\", \"2020\")"));
        assert!(params.contains("(\"offset\", \"10\")"));
    }
}
