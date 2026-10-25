use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cadence_core::model::Playlist;

use crate::ui::busy_generation::BusyGeneration;
use crate::ui::tracks::ListedSongs;

#[derive(Debug, Clone, PartialEq)]
pub enum PickedSongs {
    Ids(Vec<String>),
    Listed(ListedSongs),
}

impl Default for PickedSongs {
    fn default() -> Self {
        Self::Ids(Vec::new())
    }
}

impl PickedSongs {
    pub fn song_count(&self) -> usize {
        match self {
            Self::Ids(song_ids) => song_ids.len(),
            Self::Listed(listed) => listed.song_count(),
        }
    }
}

#[derive(Default)]
pub struct Picker {
    work: BusyGeneration,
    songs: PickedSongs,
    pub playlists: Vec<Playlist>,
}

pub type SharedPicker = Arc<Mutex<Picker>>;

pub fn lock(shared: &SharedPicker) -> MutexGuard<'_, Picker> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct PendingRequest {
    pub generation: u64,
    pub songs: PickedSongs,
}

impl Picker {
    pub fn restart(&mut self, songs: PickedSongs) -> u64 {
        let generation = self.work.restart();
        self.songs = songs;
        self.playlists = Vec::new();
        generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.work.is_current(generation)
    }

    pub fn playlist_name(&self, playlist_id: &str) -> String {
        self.playlists
            .iter()
            .find(|playlist| playlist.id == playlist_id)
            .map_or_else(|| tr!("playlist"), |playlist| playlist.name.clone())
    }

    pub fn start_request(&mut self) -> Option<PendingRequest> {
        if self.songs.song_count() == 0 || !self.work.claim() {
            return None;
        }
        Some(PendingRequest {
            generation: self.work.current(),
            songs: self.songs.clone(),
        })
    }

    pub fn finish_request(&mut self, generation: u64) {
        self.work.finish(generation);
    }
}
