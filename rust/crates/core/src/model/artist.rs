use serde::{Deserialize, Serialize};

use super::album::Album;
use super::song::Song;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

pub fn joined_artist_names(artists: &[ArtistRef]) -> String {
    artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub album_count: Option<u32>,
    pub song_count: Option<u32>,
    pub cover_art_id: Option<String>,
    #[serde(rename = "favorite")]
    pub is_favorite: bool,
    pub user_rating: Option<u8>,
    pub biography: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistDetail {
    pub artist: Artist,
    pub albums: Vec<Album>,
    pub top_songs: Vec<Song>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArtistPart {
    Profile(Artist),
    Albums(Vec<Album>),
    TopSongs(Vec<Song>),
    Biography(Option<String>),
}

impl ArtistPart {
    pub fn merge_into(self, detail: &mut Option<ArtistDetail>) {
        match (detail.as_mut(), self) {
            (Some(detail), part) => detail.absorb(part),
            (None, Self::Profile(artist)) => {
                *detail = Some(ArtistDetail {
                    artist,
                    albums: Vec::new(),
                    top_songs: Vec::new(),
                });
            }
            (None, _part_before_the_profile) => {}
        }
    }
}

impl ArtistDetail {
    fn absorb(&mut self, part: ArtistPart) {
        match part {
            ArtistPart::Profile(mut artist) => {
                artist.biography = artist.biography.or(self.artist.biography.take());
                self.artist = artist;
            }
            ArtistPart::Albums(albums) => self.albums = albums,
            ArtistPart::TopSongs(top_songs) => self.top_songs = top_songs,
            ArtistPart::Biography(biography) => self.artist.biography = biography,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(name: &str, biography: Option<&str>) -> Artist {
        Artist {
            id: "artist-1".into(),
            server_id: "server-1".into(),
            name: name.into(),
            album_count: None,
            song_count: None,
            cover_art_id: None,
            is_favorite: false,
            user_rating: None,
            biography: biography.map(Into::into),
        }
    }

    #[test]
    fn given_no_detail_when_a_part_precedes_the_profile_then_it_is_dropped() {
        let mut detail = None;
        ArtistPart::Biography(Some("Early".into())).merge_into(&mut detail);
        assert_eq!(detail, None);
    }

    #[test]
    fn given_no_detail_when_the_profile_arrives_then_it_starts_the_detail() {
        let mut detail = None;
        ArtistPart::Profile(artist("Name", None)).merge_into(&mut detail);
        let detail = detail.expect("the profile starts the detail");
        assert_eq!(detail.artist.name, "Name");
        assert!(detail.albums.is_empty());
        assert!(detail.top_songs.is_empty());
    }

    #[test]
    fn given_a_detail_when_parts_arrive_then_each_fills_its_section() {
        let mut detail = None;
        ArtistPart::Profile(artist("Name", None)).merge_into(&mut detail);
        ArtistPart::Biography(Some("Life story".into())).merge_into(&mut detail);
        ArtistPart::Albums(Vec::new()).merge_into(&mut detail);
        ArtistPart::TopSongs(Vec::new()).merge_into(&mut detail);
        let detail = detail.expect("the detail exists");
        assert_eq!(detail.artist.biography.as_deref(), Some("Life story"));
    }

    #[test]
    fn given_a_known_biography_when_a_seed_profile_arrives_then_the_biography_stays() {
        let mut detail = None;
        ArtistPart::Profile(artist("Name", Some("Life story"))).merge_into(&mut detail);
        ArtistPart::Profile(artist("Renamed", None)).merge_into(&mut detail);
        let detail = detail.expect("the detail exists");
        assert_eq!(detail.artist.name, "Renamed");
        assert_eq!(detail.artist.biography.as_deref(), Some("Life story"));
    }
}
