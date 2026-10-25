use cadence_core::duration::seconds_to_ms;
use cadence_core::model::{Album, Song};

use super::artists::{SingleArtist, artist_refs, genre_names};
use super::numbers::{capped_rating, non_empty_text, non_negative_u32, nonzero_year};
use super::song::song;
use crate::types::{ReleaseDate, SubsonicAlbum};

fn is_complete_release_date(release_date: &ReleaseDate) -> bool {
    release_date.year.is_some() && release_date.month.is_some() && release_date.day.is_some()
}

fn release_year(item: &SubsonicAlbum) -> Option<i32> {
    match &item.release_date {
        Some(release_date) if is_complete_release_date(release_date) => {
            nonzero_year(release_date.year)
                .filter(|year| *year > 0)
                .or_else(|| nonzero_year(item.year))
        }
        _ => nonzero_year(item.year),
    }
}

pub fn album(item: SubsonicAlbum, server_id: &str) -> (Album, Vec<Song>) {
    let year = release_year(&item);
    let album_artists = artist_refs(
        item.artists,
        SingleArtist {
            id: item.artist_id.as_deref(),
            name: item.artist.as_deref(),
        },
        None,
    );
    let songs = item
        .song
        .unwrap_or_default()
        .into_iter()
        .map(|entry| song(entry, server_id))
        .collect();
    let album = Album {
        id: item.id,
        server_id: server_id.to_owned(),
        name: item.name.or(item.title).or(item.album).unwrap_or_default(),
        album_artists,
        artists: Vec::new(),
        year,
        genres: genre_names(item.genres, item.genre),
        song_count: non_negative_u32(item.song_count).unwrap_or(0),
        duration_ms: item.duration.map_or(0, seconds_to_ms),
        cover_art_id: non_empty_text(item.cover_art),
        play_count: non_negative_u32(item.play_count).unwrap_or(0),
        user_rating: capped_rating(item.user_rating),
        is_favorite: item.starred.is_starred(),
        created_at: item.created,
        last_played_at: non_empty_text(item.played),
    };
    (album, songs)
}

pub fn album_without_songs(item: SubsonicAlbum, server_id: &str) -> Album {
    album(item, server_id).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn album_prefers_full_release_date_year() {
        let item: SubsonicAlbum = serde_json::from_value(json!({
            "id": "a1", "name": "N", "artist": "X", "artistId": "x", "year": 1999,
            "releaseDate": {"year": 2005, "month": 3, "day": 1}, "songCount": 3, "duration": 100,
            "song": [{"id": "s1", "title": "one"}]
        }))
        .unwrap_or_default();
        let (album, songs) = album(item, "srv");
        assert_eq!(album.year, Some(2005));
        assert_eq!(album.song_count, 3);
        assert_eq!(album.album_artists[0].id, "x");
        assert_eq!(songs.len(), 1);
    }

    #[test]
    fn given_a_release_date_with_a_negative_year_when_normalizing_then_the_album_year_is_used() {
        let item: SubsonicAlbum = serde_json::from_value(json!({
            "id": "a1", "name": "N", "year": 1999,
            "releaseDate": {"year": -1, "month": 1, "day": 1}
        }))
        .unwrap_or_default();
        assert_eq!(album(item, "srv").0.year, Some(1999));
    }

    #[test]
    fn given_a_played_timestamp_when_normalizing_an_album_then_it_is_the_last_play() {
        let item: SubsonicAlbum = serde_json::from_value(json!({
            "id": "a1", "name": "N", "played": "2024-05-06T07:08:09Z"
        }))
        .unwrap_or_default();
        let (album, _) = album(item, "srv");
        assert_eq!(
            album.last_played_at.as_deref(),
            Some("2024-05-06T07:08:09Z")
        );
    }

    #[test]
    fn given_an_empty_played_timestamp_when_normalizing_an_album_then_it_was_never_played() {
        let item: SubsonicAlbum =
            serde_json::from_value(json!({"id": "a1", "name": "N", "played": ""}))
                .unwrap_or_default();
        assert_eq!(album(item, "srv").0.last_played_at, None);
    }
}
