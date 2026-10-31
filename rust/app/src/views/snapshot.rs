use cadence_core::model::{
    Album, AlbumDetail, Artist, ArtistDetail, ArtistPart, Folder, Genre, Page, Playlist, Song,
};
use cadence_youtube::music::CollectionPage;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::edits::Edits;

pub trait Overlay {
    fn overlay(&mut self, _edits: &Edits<'_>) {}
    fn drop_live_fields(&mut self) {}
}

pub trait Snapshot:
    Overlay + Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
}

impl<Payload> Snapshot for Payload where
    Payload: Overlay + Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
}

impl Overlay for usize {}
impl Overlay for bool {}
impl Overlay for Genre {}
impl Overlay for Playlist {}
impl Overlay for Folder {}
impl Overlay for CollectionPage {}

impl Overlay for Song {
    fn overlay(&mut self, edits: &Edits<'_>) {
        edits.apply(self);
    }
}

impl Overlay for Album {
    fn overlay(&mut self, edits: &Edits<'_>) {
        edits.apply(self);
    }
}

impl Overlay for Artist {
    fn overlay(&mut self, edits: &Edits<'_>) {
        edits.apply(self);
    }
}

impl<Item: Overlay> Overlay for Vec<Item> {
    fn overlay(&mut self, edits: &Edits<'_>) {
        for item in self {
            item.overlay(edits);
        }
    }
}

impl<Item: Overlay> Overlay for Page<Item> {
    fn overlay(&mut self, edits: &Edits<'_>) {
        self.items.overlay(edits);
    }

    fn drop_live_fields(&mut self) {
        self.total_count = None;
    }
}

impl<Item: Overlay> Overlay for Option<Item> {
    fn overlay(&mut self, edits: &Edits<'_>) {
        if let Some(item) = self {
            item.overlay(edits);
        }
    }
}

impl<First: Overlay, Second: Overlay> Overlay for (First, Second) {
    fn overlay(&mut self, edits: &Edits<'_>) {
        self.0.overlay(edits);
        self.1.overlay(edits);
    }
}

impl Overlay for AlbumDetail {
    fn overlay(&mut self, edits: &Edits<'_>) {
        self.album.overlay(edits);
        self.songs.overlay(edits);
    }
}

impl Overlay for ArtistDetail {
    fn overlay(&mut self, edits: &Edits<'_>) {
        self.artist.overlay(edits);
        self.albums.overlay(edits);
        self.top_songs.overlay(edits);
    }
}

impl Overlay for ArtistPart {
    fn overlay(&mut self, edits: &Edits<'_>) {
        match self {
            Self::Profile(artist) => artist.overlay(edits),
            Self::Albums(albums) => albums.overlay(edits),
            Self::TopSongs(songs) => songs.overlay(edits),
            Self::Biography(_) => {}
        }
    }
}
