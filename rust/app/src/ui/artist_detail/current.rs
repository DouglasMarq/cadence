use std::cell::RefCell;
use std::rc::Rc;

use cadence_core::model::{ArtistDetail, Song};
use slint::{Image, VecModel};

use super::super::cover_lru::CoverLru;
use super::super::{ArtistAlbumCard, TrackRow};

const ALBUM_COVER_CAPACITY: usize = 48;

pub struct CurrentArtist {
    pub generation: u64,
    pub id: String,
    pub detail: Option<ArtistDetail>,
    pub image_cover_id: Option<String>,
    pub covers: CoverLru<usize, Image>,
    pub top_rows: Rc<VecModel<TrackRow>>,
    pub album_cards: Rc<VecModel<ArtistAlbumCard>>,
}

impl Default for CurrentArtist {
    fn default() -> Self {
        Self {
            generation: 0,
            id: String::new(),
            detail: None,
            image_cover_id: None,
            covers: CoverLru::new(ALBUM_COVER_CAPACITY),
            top_rows: Rc::default(),
            album_cards: Rc::default(),
        }
    }
}

impl CurrentArtist {
    pub fn top_songs(&self) -> Vec<Song> {
        self.detail
            .as_ref()
            .map(|detail| detail.top_songs.clone())
            .unwrap_or_default()
    }

    pub fn album_cover_id(&self, index: usize) -> Option<String> {
        self.detail
            .as_ref()?
            .albums
            .get(index)?
            .cover_art_id
            .clone()
    }
}

thread_local! {
    static CURRENT_ARTIST: RefCell<CurrentArtist> = RefCell::default();
}

pub fn read_current<Output>(action: impl FnOnce(&CurrentArtist) -> Output) -> Output {
    CURRENT_ARTIST.with_borrow(action)
}

pub fn update_current<Output>(action: impl FnOnce(&mut CurrentArtist) -> Output) -> Output {
    CURRENT_ARTIST.with_borrow_mut(action)
}

pub fn begin(artist_id: &str) -> Option<u64> {
    update_current(|current| {
        if artist_id.is_empty() {
            return None;
        }
        *current = CurrentArtist {
            generation: current.generation + 1,
            id: artist_id.to_owned(),
            ..CurrentArtist::default()
        };
        Some(current.generation)
    })
}

pub fn is_current(generation: u64) -> bool {
    read_current(|current| current.generation == generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generations_guard_navigation() {
        let first = begin("artist-1");
        let second = begin("artist-2");
        assert!(first.is_some() && second > first);
        assert!(first.is_some_and(|generation| !is_current(generation)));
        assert!(second.is_some_and(is_current));
        assert_eq!(begin(""), None);
    }

    #[test]
    fn given_the_same_artist_is_opened_again_it_reloads() {
        let first = begin("artist-1");
        let again = begin("artist-1");
        assert!(again > first);
        assert!(again.is_some_and(is_current));
    }
}
