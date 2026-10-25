use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::ui::busy_generation::BusyGeneration;

#[derive(Default)]
pub struct Detail {
    work: BusyGeneration,
    pub id: String,
    pub name: String,
}

pub type SharedDetail = Arc<Mutex<Detail>>;

pub fn lock(shared: &SharedDetail) -> MutexGuard<'_, Detail> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct RemovalTarget {
    pub generation: u64,
    pub playlist_id: String,
    pub name: String,
}

impl Detail {
    pub fn restart(&mut self, playlist_id: &str) -> u64 {
        let generation = self.work.restart();
        self.id = playlist_id.to_owned();
        self.name = String::new();
        generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.work.is_current(generation)
    }

    pub fn start_removal(&mut self, indexes: &[usize]) -> Option<RemovalTarget> {
        if indexes.is_empty() || self.id.is_empty() || !self.work.claim() {
            return None;
        }
        Some(RemovalTarget {
            generation: self.work.current(),
            playlist_id: self.id.clone(),
            name: self.name.clone(),
        })
    }

    pub fn finish_removal(&mut self, generation: u64) {
        self.work.finish(generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removals_do_not_overlap() {
        let mut detail = Detail::default();
        let generation = detail.restart("playlist-1");
        assert!(detail.start_removal(&[]).is_none());
        assert!(detail.start_removal(&[1]).is_some());
        assert!(detail.start_removal(&[2]).is_none());
        assert!(!detail.is_current(generation + 1));
        detail.finish_removal(generation + 1);
        assert!(detail.start_removal(&[2]).is_none());
        assert!(detail.is_current(generation));
        detail.finish_removal(generation);
        assert!(detail.start_removal(&[2]).is_some());
    }
}
