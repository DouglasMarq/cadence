use std::collections::VecDeque;
use std::ops::Range;

use cadence_api::MusicServer;
use cadence_core::model::{Paging, Song};

use crate::library::song_page;
use crate::list_stream::RowRanges;
use crate::list_stream::SongSource;

pub const LISTED_CHUNK_SIZE: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct ListedSongs {
    pub source: SongSource,
    pub ranges: RowRanges,
}

impl ListedSongs {
    pub fn song_count(&self) -> usize {
        self.ranges.row_count()
    }

    pub fn into_chunks(self, chunk_size: usize) -> ListedChunks {
        ListedChunks {
            source: self.source,
            pending: self.ranges.into_ranges().into(),
            chunk_size: chunk_size.max(1),
        }
    }
}

pub struct ListedChunks {
    source: SongSource,
    pending: VecDeque<Range<usize>>,
    chunk_size: usize,
}

impl ListedChunks {
    pub async fn next_songs(
        &mut self,
        server: &dyn MusicServer,
    ) -> cadence_api::Result<Option<Vec<Song>>> {
        let Some(rows) = self.next_rows() else {
            return Ok(None);
        };
        let paging = Paging {
            start_index: rows.start,
            limit: rows.len(),
        };
        let songs = song_page(server, &self.source, paging).await?.items;
        if songs.len() < rows.len() {
            self.pending.clear();
        }
        Ok(Some(songs))
    }

    pub async fn next_ids(
        &mut self,
        server: &dyn MusicServer,
    ) -> cadence_api::Result<Option<Vec<String>>> {
        let songs = self.next_songs(server).await?;
        Ok(songs.map(|songs| songs.into_iter().map(|song| song.id).collect()))
    }

    fn next_rows(&mut self) -> Option<Range<usize>> {
        let rows = self.pending.pop_front()?;
        let end = rows.start.saturating_add(self.chunk_size).min(rows.end);
        if end < rows.end {
            self.pending.push_front(end..rows.end);
        }
        Some(rows.start..end)
    }
}
