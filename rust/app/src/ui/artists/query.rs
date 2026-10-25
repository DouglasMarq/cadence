use cadence_core::model::{ArtistListQuery, ArtistSort};

use crate::ui::{ListControls, normalized_search, sort_at, sort_order};

pub const SORTS: [ArtistSort; 7] = [
    ArtistSort::Name,
    ArtistSort::AlbumCount,
    ArtistSort::SongCount,
    ArtistSort::PlayCount,
    ArtistSort::Rating,
    ArtistSort::Favorited,
    ArtistSort::Random,
];

pub fn sort_label(sort: ArtistSort) -> String {
    match sort {
        ArtistSort::Name => tr!("Name"),
        ArtistSort::AlbumCount => tr!("Album count"),
        ArtistSort::SongCount => tr!("Song count"),
        ArtistSort::PlayCount => tr!("Play count"),
        ArtistSort::Rating => tr!("Rating"),
        ArtistSort::Favorited => tr!("Favorited"),
        ArtistSort::Random => tr!("Random"),
    }
}

pub fn build_query(controls: &ListControls) -> ArtistListQuery {
    ArtistListQuery {
        sort: sort_at(&SORTS, controls.sort_index, ArtistSort::Name),
        order: sort_order(controls.is_descending),
        search: normalized_search(controls.search),
        is_favorite: controls.is_favorites_only.then_some(true),
        music_folder_id: None,
    }
}

#[cfg(test)]
mod tests {
    use cadence_core::model::SortOrder;

    use super::*;

    #[test]
    fn query_follows_the_controls() {
        let query = build_query(&ListControls {
            sort_index: 2,
            is_descending: true,
            search: "  vale ",
            is_favorites_only: true,
        });
        assert_eq!(query.sort, ArtistSort::SongCount);
        assert_eq!(query.order, SortOrder::Desc);
        assert_eq!(query.search.as_deref(), Some("vale"));
        assert_eq!(query.is_favorite, Some(true));
        let query = build_query(&ListControls {
            sort_index: -1,
            is_descending: false,
            search: " ",
            is_favorites_only: false,
        });
        assert_eq!(query, ArtistListQuery::default());
        let unknown_sort = ListControls {
            sort_index: 99,
            is_descending: false,
            search: "",
            is_favorites_only: false,
        };
        assert_eq!(build_query(&unknown_sort).sort, ArtistSort::Name);
    }
}
