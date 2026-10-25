use std::collections::HashSet;

use cadence_api::{ApiError, MusicServer};

use super::playlist_writes::exclusive_playlist_write;
use crate::list_stream::{PlaylistId, RowRanges, SongSource};
use crate::ui::tracks::{LISTED_CHUNK_SIZE, ListedChunks, ListedSongs};

#[derive(Debug, Default)]
pub struct ListedAdd {
    pub added: usize,
    pub read: usize,
    pub failure: Option<ApiError>,
}

impl ListedAdd {
    pub fn failed(error: ApiError) -> Self {
        Self {
            failure: Some(error),
            ..Self::default()
        }
    }
}

pub struct ListedWrite<'write, Report> {
    pub server: &'write dyn MusicServer,
    pub playlist_id: &'write str,
    pub report: Report,
}

struct ChunkAdded {
    read: usize,
    added: usize,
}

pub async fn existing_song_ids(
    server: &dyn MusicServer,
    playlist_id: &str,
) -> cadence_api::Result<HashSet<String>> {
    let whole_playlist = ListedSongs {
        source: SongSource::Playlist(PlaylistId(playlist_id.to_owned())),
        ranges: RowRanges::spanning(RowRanges::WHOLE_LIST),
    };
    let mut chunks = whole_playlist.into_chunks(LISTED_CHUNK_SIZE);
    let mut existing = HashSet::new();
    while let Some(song_ids) = chunks.next_ids(server).await? {
        existing.extend(song_ids);
    }
    Ok(existing)
}

pub async fn add_listed_to_existing(
    write: &ListedWrite<'_, impl Fn(usize)>,
    listed: ListedSongs,
) -> ListedAdd {
    let _exclusive = exclusive_playlist_write(write.playlist_id).await;
    match existing_song_ids(write.server, write.playlist_id).await {
        Ok(existing) => add_listed_songs(write, listed, existing).await,
        Err(error) => ListedAdd::failed(error),
    }
}

pub async fn add_listed_songs(
    write: &ListedWrite<'_, impl Fn(usize)>,
    listed: ListedSongs,
    mut known: HashSet<String>,
) -> ListedAdd {
    let mut chunks = listed.into_chunks(LISTED_CHUNK_SIZE);
    let mut outcome = ListedAdd::default();
    loop {
        match add_next_chunk(write, &mut chunks, &mut known).await {
            Ok(Some(chunk)) => {
                outcome.read += chunk.read;
                outcome.added += chunk.added;
                (write.report)(outcome.read);
            }
            Ok(None) => return outcome,
            Err(error) => {
                outcome.failure = Some(error);
                return outcome;
            }
        }
    }
}

async fn add_next_chunk<Report>(
    write: &ListedWrite<'_, Report>,
    chunks: &mut ListedChunks,
    known: &mut HashSet<String>,
) -> cadence_api::Result<Option<ChunkAdded>> {
    let Some(song_ids) = chunks.next_ids(write.server).await? else {
        return Ok(None);
    };
    let read = song_ids.len();
    let new_ids = unseen_ids(known, song_ids);
    if !new_ids.is_empty() {
        write
            .server
            .add_to_playlist(write.playlist_id, &new_ids)
            .await?;
    }
    Ok(Some(ChunkAdded {
        read,
        added: new_ids.len(),
    }))
}

fn unseen_ids(known: &mut HashSet<String>, song_ids: Vec<String>) -> Vec<String> {
    song_ids
        .into_iter()
        .filter(|song_id| known.insert(song_id.clone()))
        .collect()
}
