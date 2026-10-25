use cadence_core::model::Song;

use super::{LOADED_SELECTION_LIMIT, TracksModel};
use crate::list_stream::{ListRow, RowRanges, SongSource};
use crate::ui::tracks::listed::ListedSongs;
use crate::ui::tracks::paged_songs::SongPages;
use crate::ui::tracks::selection::{FirstSelected, SelectedSongs, Selection};

fn loaded_selection(songs: &SongPages) -> Option<Vec<Song>> {
    let ranges = songs.selection().ranges();
    if ranges.row_count() > LOADED_SELECTION_LIMIT {
        return None;
    }
    ranges.rows().map(|row| songs.song(row).cloned()).collect()
}

impl TracksModel {
    pub fn select(&self, index: usize, is_toggling: bool, is_extending: bool) {
        self.songs.change_selection(Some(index), |selection, _| {
            selection.click(index, is_toggling, is_extending);
        });
    }

    pub fn focus(&self, index: usize) {
        self.songs
            .change_selection(Some(index), |selection, _| selection.focus(index));
    }

    pub fn select_all(&self) {
        self.songs.change_selection(None, Selection::select_all);
    }

    pub fn clear_selection(&self) {
        self.songs
            .change_selection(None, |selection, _| selection.clear());
    }

    pub fn selected_count(&self) -> usize {
        self.songs.selected_count()
    }

    fn library_source(&self) -> SongSource {
        SongSource::Library(self.query())
    }

    pub fn selected_songs(&self) -> SelectedSongs {
        let songs = self.songs.read();
        loaded_selection(&songs).map_or_else(
            || {
                SelectedSongs::Listed(ListedSongs {
                    source: self.library_source(),
                    ranges: songs.selection().ranges().clone(),
                })
            },
            SelectedSongs::Loaded,
        )
    }

    pub fn first_selected(&self) -> Option<FirstSelected> {
        let songs = self.songs.read();
        let ListRow(row) = songs.selection().ranges().first_row()?;
        Some(FirstSelected {
            listed: ListedSongs {
                source: self.library_source(),
                ranges: RowRanges::spanning(row..row.saturating_add(1)),
            },
            song: songs.song(row).cloned(),
        })
    }
}
