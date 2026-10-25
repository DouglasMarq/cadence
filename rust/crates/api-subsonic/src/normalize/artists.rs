use cadence_core::model::ArtistRef;

use super::numbers::non_empty_text;
use crate::types::{Contributor, GenreItem, SimpleArtist};

fn simple_artist_refs(artists: Vec<SimpleArtist>) -> Vec<ArtistRef> {
    artists
        .into_iter()
        .map(|artist| ArtistRef {
            id: artist.id,
            name: artist.name,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct SingleArtist<'item> {
    pub id: Option<&'item str>,
    pub name: Option<&'item str>,
}

fn single_artist_ref(single: SingleArtist) -> Option<ArtistRef> {
    let id = single.id.unwrap_or_default();
    let name = single.name.unwrap_or_default();
    (!id.is_empty() || !name.is_empty()).then(|| ArtistRef {
        id: id.to_owned(),
        name: name.to_owned(),
    })
}

fn is_plain_remixer(contributor: &Contributor) -> bool {
    contributor.role == "remixer" && contributor.sub_role.is_none()
}

fn with_remixers(mut artists: Vec<ArtistRef>, contributors: &[Contributor]) -> Vec<ArtistRef> {
    for contributor in contributors
        .iter()
        .filter(|contributor| is_plain_remixer(contributor))
    {
        let already_listed = artists
            .iter()
            .any(|artist| artist.id == contributor.artist.id);
        if !already_listed {
            artists.push(ArtistRef {
                id: contributor.artist.id.clone(),
                name: contributor.artist.name.clone(),
            });
        }
    }
    artists
}

pub fn artist_refs(
    artists: Option<Vec<SimpleArtist>>,
    single: SingleArtist,
    contributors: Option<&[Contributor]>,
) -> Vec<ArtistRef> {
    match (artists, contributors) {
        (None, None) => single_artist_ref(single).into_iter().collect(),
        (artists, contributors) => with_remixers(
            simple_artist_refs(artists.unwrap_or_default()),
            contributors.unwrap_or_default(),
        ),
    }
}

pub fn genre_names(genres: Option<Vec<GenreItem>>, genre: Option<String>) -> Vec<String> {
    match genres {
        Some(list) => list.into_iter().map(|item| item.name).collect(),
        None => non_empty_text(genre).into_iter().collect(),
    }
}
