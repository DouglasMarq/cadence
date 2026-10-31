use cadence_core::model::{ItemKind, SongListQuery, SongSort};
use slint::ComponentHandle;

use super::super::artists;
use super::super::{AppWindow, ArtistDetailState, Context, PlayerState, on_ui, show_status};
use super::current::{is_current, update_current};
use crate::controller::Command;
use crate::list_stream::{ListRow, Placement, RowRanges, SongSource};

pub fn artist_songs_query(artist_id: &str) -> SongListQuery {
    SongListQuery {
        sort: SongSort::Album,
        artist_ids: vec![artist_id.to_owned()],
        ..SongListQuery::default()
    }
}

pub fn play_artist(context: &Context, artist_id: &str) {
    context.send(Command::PlayFromList {
        source: SongSource::Library(artist_songs_query(artist_id)),
        song_id: None,
        row_index: ListRow(0),
    });
}

pub fn shuffle_artist(window: &AppWindow, context: &Context, artist_id: &str) {
    if !window.global::<PlayerState>().get_is_shuffled() {
        context.send(Command::ToggleShuffle);
    }
    context.send(Command::EnqueueFromList {
        source: SongSource::Library(artist_songs_query(artist_id)),
        ranges: vec![RowRanges::WHOLE_LIST],
        placement: Placement::Now,
    });
}

struct FavoriteFlip {
    generation: u64,
    artist_id: String,
    is_favorite: bool,
}

fn flip_loaded_favorite() -> Option<FavoriteFlip> {
    update_current(|current| {
        let artist = &mut current.detail.as_mut()?.artist;
        artist.is_favorite = !artist.is_favorite;
        let is_favorite = artist.is_favorite;
        Some(FavoriteFlip {
            generation: current.generation,
            artist_id: current.id.clone(),
            is_favorite,
        })
    })
}

fn remember_favorite(is_favorite: bool) {
    update_current(|current| {
        if let Some(detail) = current.detail.as_mut() {
            detail.artist.is_favorite = is_favorite;
        }
    });
}

fn revert_favorite(window: &AppWindow, flip: &FavoriteFlip) {
    let FavoriteFlip {
        generation,
        ref artist_id,
        is_favorite,
    } = *flip;
    artists::set_favorite(window, artist_id, !is_favorite);
    if is_current(generation) {
        remember_favorite(!is_favorite);
        window
            .global::<ArtistDetailState>()
            .set_is_favorite(!is_favorite);
    }
}

pub fn toggle_favorite(window: &AppWindow, context: &Context) {
    let Some(flip) = flip_loaded_favorite() else {
        return;
    };
    let Some(server) = context.session.get() else {
        return;
    };
    window
        .global::<ArtistDetailState>()
        .set_is_favorite(flip.is_favorite);
    artists::set_favorite(window, &flip.artist_id, flip.is_favorite);
    let weak_window = window.as_weak();
    let views = context.views(&server);
    context.runtime.spawn(async move {
        let artist_ids = [flip.artist_id.clone()];
        let result = views
            .set_favorite(ItemKind::Artist, &artist_ids, flip.is_favorite)
            .await;
        if let Err(error) = result {
            on_ui(&weak_window, move |window| {
                revert_favorite(window, &flip);
                show_status(window, &tr!("Could not update the favorite: {}", error));
            });
        }
    });
}

pub fn play_album(window: &AppWindow, context: &Context, album_id: String) {
    let Some(server) = context.session.get() else {
        return;
    };
    let commands = context.clone();
    let weak_window = window.as_weak();
    context.runtime.spawn(async move {
        match server.album(&album_id).await {
            Ok(album) => commands.send(Command::PlaySongs {
                songs: album.songs,
                song_id: None,
            }),
            Err(error) => on_ui(&weak_window, move |window| {
                show_status(window, &tr!("Could not play the album: {}", error));
            }),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_all_queries_every_song_of_the_artist() {
        let query = artist_songs_query("artist-7");
        assert_eq!(query.artist_ids, ["artist-7"]);
        assert_eq!(query.sort, SongSort::Album);
        assert!(query.search.is_none() && query.album_ids.is_empty());
    }
}
