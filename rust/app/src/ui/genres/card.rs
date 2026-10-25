use cadence_api::MusicServer;
use cadence_core::model::{Album, AlbumListQuery, Artist, ArtistListQuery, ItemKind, Paging};

use crate::ui::album_grid::credited_artist_names;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardData {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover_id: Option<String>,
    pub is_favorite: bool,
}

pub struct CardPage {
    pub cards: Vec<CardData>,
    pub total: Option<usize>,
}

pub fn album_count_label(album_count: Option<u32>) -> String {
    match album_count {
        Some(count) => tr_plural!("{n} album", "{n} albums", count),
        None => String::new(),
    }
}

pub fn album_card(album: &Album) -> CardData {
    CardData {
        id: album.id.clone(),
        title: album.name.clone(),
        subtitle: credited_artist_names(album),
        cover_id: album.cover_art_id.clone(),
        is_favorite: album.is_favorite,
    }
}

pub fn artist_card(artist: &Artist) -> CardData {
    CardData {
        id: artist.id.clone(),
        title: artist.name.clone(),
        subtitle: album_count_label(artist.album_count),
        cover_id: artist.cover_art_id.clone(),
        is_favorite: artist.is_favorite,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardSource {
    Albums(AlbumListQuery),
    Artists(ArtistListQuery),
}

impl CardSource {
    pub fn kind(&self) -> ItemKind {
        match self {
            Self::Albums(_) => ItemKind::Album,
            Self::Artists(_) => ItemKind::Artist,
        }
    }

    pub async fn fetch(
        &self,
        server: &dyn MusicServer,
        paging: Paging,
    ) -> cadence_api::Result<CardPage> {
        Ok(match self {
            Self::Albums(query) => {
                let page = server.album_list(query, paging).await?;
                CardPage {
                    cards: page.items.iter().map(album_card).collect(),
                    total: page.total_count,
                }
            }
            Self::Artists(query) => {
                let page = server.artist_list(query, paging).await?;
                CardPage {
                    cards: page.items.iter().map(artist_card).collect(),
                    total: page.total_count,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use cadence_core::model::ArtistRef;

    use super::*;

    fn album_by_ada() -> Album {
        Album {
            id: "al".into(),
            server_id: "s".into(),
            name: "Name".into(),
            album_artists: vec![ArtistRef {
                id: "ar".into(),
                name: "Ada".into(),
            }],
            artists: Vec::new(),
            year: None,
            genres: Vec::new(),
            song_count: 1,
            duration_ms: 0,
            cover_art_id: Some("c".into()),
            play_count: 0,
            user_rating: None,
            is_favorite: true,
            created_at: None,
            last_played_at: None,
        }
    }

    fn artist_with_three_albums() -> Artist {
        Artist {
            id: "ar".into(),
            server_id: "s".into(),
            name: "Ada".into(),
            album_count: Some(3),
            song_count: None,
            cover_art_id: None,
            is_favorite: false,
            user_rating: None,
            biography: None,
        }
    }

    #[test]
    fn cards_map_albums_and_artists() {
        let card = album_card(&album_by_ada());
        assert_eq!(
            (
                card.subtitle.as_str(),
                card.cover_id.as_deref(),
                card.is_favorite
            ),
            ("Ada", Some("c"), true)
        );
        assert_eq!(
            artist_card(&artist_with_three_albums()).subtitle,
            "3 albums"
        );
    }
}
