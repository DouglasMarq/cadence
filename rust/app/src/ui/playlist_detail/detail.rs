use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cadence_core::model::Playlist;

use crate::ui::busy_generation::BusyGeneration;

#[derive(Default)]
pub struct Detail {
    work: BusyGeneration,
    pub id: String,
    pub name: String,
    pub metadata: Option<Playlist>,
    reported_total: Option<usize>,
    has_header_answer: bool,
    has_first_page_answer: bool,
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
        self.metadata = None;
        self.reported_total = None;
        self.has_header_answer = false;
        self.has_first_page_answer = false;
        generation
    }

    pub fn adopt_metadata(&mut self, metadata: Option<Playlist>) {
        self.name = metadata
            .as_ref()
            .map_or_else(|| tr!("Playlist"), |playlist| playlist.name.clone());
        self.metadata = metadata;
        self.has_header_answer = true;
    }

    pub fn adopt_first_page(&mut self, reported_total: Option<usize>, song_count: usize) -> usize {
        self.reported_total = reported_total;
        self.has_first_page_answer = true;
        reported_total
            .or_else(|| self.metadata_song_count())
            .unwrap_or(song_count)
    }

    pub fn first_page_failed(&mut self) {
        self.has_first_page_answer = true;
        self.reported_total = Some(0);
    }

    pub fn is_settled(&self) -> bool {
        self.has_header_answer && self.has_first_page_answer
    }

    pub fn total_without_a_page_report(&self) -> Option<usize> {
        self.metadata_song_count()
            .filter(|_| self.has_first_page_answer && self.reported_total.is_none())
    }

    fn metadata_song_count(&self) -> Option<usize> {
        self.metadata
            .as_ref()
            .map(|playlist| playlist.song_count as usize)
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

    fn playlist(song_count: u32) -> Playlist {
        Playlist {
            id: "playlist-1".into(),
            server_id: "demo".into(),
            name: "Mine".into(),
            comment: None,
            owner: None,
            is_public: false,
            song_count,
            duration_ms: 0,
            cover_art_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn given_a_page_without_a_total_when_the_metadata_comes_later_then_it_gives_the_total() {
        let mut detail = Detail::default();
        detail.restart("playlist-1");
        assert_eq!(detail.adopt_first_page(None, 100), 100);
        assert_eq!(detail.total_without_a_page_report(), None);

        detail.adopt_metadata(Some(playlist(250)));

        assert_eq!(detail.total_without_a_page_report(), Some(250));
        assert!(detail.is_settled());
    }

    #[test]
    fn given_the_metadata_first_when_a_page_without_a_total_arrives_then_it_uses_the_metadata() {
        let mut detail = Detail::default();
        detail.restart("playlist-1");
        detail.adopt_metadata(Some(playlist(250)));
        assert!(!detail.is_settled());

        assert_eq!(detail.adopt_first_page(None, 100), 250);
    }

    #[test]
    fn given_a_page_reporting_its_total_when_the_metadata_arrives_then_the_page_total_stays() {
        let mut detail = Detail::default();
        detail.restart("playlist-1");
        assert_eq!(detail.adopt_first_page(Some(120), 100), 120);

        detail.adopt_metadata(Some(playlist(250)));

        assert_eq!(detail.total_without_a_page_report(), None);
    }

    #[test]
    fn given_a_failed_first_page_when_the_metadata_arrives_then_no_rows_appear() {
        let mut detail = Detail::default();
        detail.restart("playlist-1");
        detail.first_page_failed();

        detail.adopt_metadata(Some(playlist(250)));

        assert_eq!(detail.total_without_a_page_report(), None);
        assert!(detail.is_settled());
    }

    #[test]
    fn given_no_metadata_when_it_fails_then_the_name_falls_back() {
        let mut detail = Detail::default();
        detail.restart("playlist-1");

        detail.adopt_metadata(None);

        assert_eq!(detail.name, "Playlist");
    }

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
