use cadence_core::model::Song;

use super::listed::ListedSongs;
use crate::list_stream::{ListRow, RowRanges};

#[derive(Debug, Default, Clone)]
pub struct Selection {
    ranges: RowRanges,
    anchor: Option<usize>,
}

impl Selection {
    pub fn contains(&self, index: usize) -> bool {
        self.ranges.contains(ListRow(index))
    }

    pub fn len(&self) -> usize {
        self.ranges.row_count()
    }

    pub fn ranges(&self) -> &RowRanges {
        &self.ranges
    }

    pub fn indexes(&self) -> Vec<usize> {
        self.ranges.rows().collect()
    }

    pub fn targets(&self, index: usize) -> Vec<usize> {
        if !self.contains(index) {
            return vec![index];
        }
        self.indexes()
    }

    pub fn click(&mut self, index: usize, is_toggling: bool, is_extending: bool) {
        match self.anchor.filter(|_| is_extending) {
            Some(anchor) => self.extend_from_anchor(anchor, index, is_toggling),
            None if is_toggling => self.toggle_row(index),
            None => self.select_only(index),
        }
    }

    pub fn focus(&mut self, index: usize) {
        if !self.contains(index) {
            self.click(index, false, false);
        }
    }

    pub fn select_all(&mut self, row_count: usize) {
        self.ranges = RowRanges::spanning(0..row_count);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn extend_from_anchor(&mut self, anchor: usize, index: usize, should_keep_existing: bool) {
        if !should_keep_existing {
            self.ranges = RowRanges::default();
        }
        self.ranges
            .insert(anchor.min(index)..anchor.max(index).saturating_add(1));
    }

    fn toggle_row(&mut self, index: usize) {
        self.ranges.toggle(ListRow(index));
        self.anchor = Some(index);
    }

    fn select_only(&mut self, index: usize) {
        self.ranges = RowRanges::spanning(index..index.saturating_add(1));
        self.anchor = Some(index);
    }
}

#[derive(Debug, Clone)]
pub enum SelectedSongs {
    Loaded(Vec<Song>),
    Listed(ListedSongs),
}

#[derive(Debug, Clone)]
pub struct FirstSelected {
    pub listed: ListedSongs,
    pub song: Option<Song>,
}
