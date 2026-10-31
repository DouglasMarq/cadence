use std::fmt::Debug;

use cadence_core::model::{
    AlbumListQuery, AlbumSort, ArtistListQuery, ArtistSort, SongListQuery, SongSort,
};

use super::view::{View, ViewSlot};

pub trait ListQuery: Debug {
    fn is_repeatable(&self) -> bool;
    fn is_favorites_only(&self) -> bool;
    fn library_view(&self) -> View;

    fn view(&self) -> View {
        match self.is_favorites_only() {
            true => View::Favorites,
            false => self.library_view(),
        }
    }
}

impl ListQuery for SongListQuery {
    fn is_repeatable(&self) -> bool {
        self.search.is_none() && self.sort != SongSort::Random
    }

    fn is_favorites_only(&self) -> bool {
        self.is_favorite == Some(true)
    }

    fn library_view(&self) -> View {
        View::Tracks
    }
}

impl ListQuery for AlbumListQuery {
    fn is_repeatable(&self) -> bool {
        self.search.is_none() && self.sort != AlbumSort::Random
    }

    fn is_favorites_only(&self) -> bool {
        self.is_favorite == Some(true)
    }

    fn library_view(&self) -> View {
        View::Albums
    }
}

impl ListQuery for ArtistListQuery {
    fn is_repeatable(&self) -> bool {
        self.search.is_none() && self.sort != ArtistSort::Random
    }

    fn is_favorites_only(&self) -> bool {
        self.is_favorite == Some(true)
    }

    fn library_view(&self) -> View {
        View::Artists
    }
}

pub fn count_slot<Query: ListQuery>(query: &Query) -> Option<ViewSlot> {
    query
        .is_repeatable()
        .then(|| query.view().slot(format!("count/{query:?}")))
}

pub fn page_slot<Query: ListQuery>(query: &Query, page: usize) -> Option<ViewSlot> {
    query
        .is_repeatable()
        .then(|| query.view().slot(format!("page/{page}/{query:?}")))
}

pub fn shelf_slot(query: &AlbumListQuery) -> Option<ViewSlot> {
    query
        .is_repeatable()
        .then(|| View::Home.slot(format!("shelf/{query:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_random_shelf_when_slotted_then_it_is_never_cached() {
        let random = AlbumListQuery {
            sort: AlbumSort::Random,
            ..AlbumListQuery::default()
        };
        assert_eq!(shelf_slot(&random), None);
        assert_ne!(
            shelf_slot(&AlbumListQuery::default()),
            page_slot(&AlbumListQuery::default(), 0)
        );
    }

    #[test]
    fn given_a_search_when_slotted_then_it_is_never_cached() {
        let query = SongListQuery {
            search: Some("private words".to_owned()),
            ..SongListQuery::default()
        };
        assert_eq!(count_slot(&query), None);
        assert_eq!(page_slot(&query, 0), None);
    }

    #[test]
    fn given_a_random_order_when_slotted_then_it_is_never_cached() {
        let query = AlbumListQuery {
            sort: AlbumSort::Random,
            ..AlbumListQuery::default()
        };
        assert_eq!(page_slot(&query, 0), None);
        let query = ArtistListQuery {
            sort: ArtistSort::Random,
            ..ArtistListQuery::default()
        };
        assert_eq!(page_slot(&query, 0), None);
    }

    #[test]
    fn given_a_favorites_query_when_slotted_then_it_belongs_to_the_favorites_group() {
        let query = ArtistListQuery {
            is_favorite: Some(true),
            ..ArtistListQuery::default()
        };
        assert_eq!(query.view(), View::Favorites);
        assert_eq!(ArtistListQuery::default().view(), View::Artists);
    }

    #[test]
    fn given_two_pages_when_slotted_then_their_slots_differ() {
        let query = SongListQuery::default();
        assert_ne!(page_slot(&query, 0), page_slot(&query, 1));
        assert_ne!(page_slot(&query, 0), count_slot(&query));
    }
}
