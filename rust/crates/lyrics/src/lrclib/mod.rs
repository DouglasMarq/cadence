use std::time::Duration;

use cadence_core::duration::ms_to_seconds;
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};

use crate::fuzzy::{SearchItem, close_matches};
use crate::http::send_json;
use crate::lrc::lyrics_from_text;
use crate::{Lyrics, LyricsQuery, LyricsSource, Result};

const AGENT: &str = "Cadence (https://github.com/DouglasMarq/cadence)";
const TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_SIDE_DURATION_TOLERANCE_SECS: f64 = 2.0;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Track {
    #[serde(default)]
    name: String,
    #[serde(default)]
    artist_name: String,
    duration: Option<f64>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

#[derive(Serialize)]
struct GetParams<'query> {
    track_name: &'query str,
    artist_name: &'query str,
    album_name: &'query str,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration: Option<String>,
}

pub(crate) async fn fetch(
    client: &reqwest::Client,
    base: &str,
    query: &LyricsQuery,
) -> Result<Option<Lyrics>> {
    if query.has_no_search_terms() {
        return Ok(None);
    }
    let exact_match = fetch_exact_match(client, base, query).await?;
    if exact_match.as_ref().is_some_and(|lyrics| lyrics.is_synced) {
        return Ok(exact_match);
    }
    let tracks = search_tracks(client, base, query).await?;
    Ok(best_search_match(&tracks, query, exact_match))
}

async fn fetch_exact_match(
    client: &reqwest::Client,
    base: &str,
    query: &LyricsQuery,
) -> Result<Option<Lyrics>> {
    if query.title.is_empty() {
        return Ok(None);
    }
    let album = query
        .album
        .as_deref()
        .filter(|album| !album.is_empty())
        .unwrap_or(&query.title);
    let request = client
        .get(format!("{base}/api/get"))
        .header(USER_AGENT, AGENT)
        .timeout(TIMEOUT)
        .query(&GetParams {
            track_name: &query.title,
            artist_name: &query.artist,
            album_name: album,
            duration: duration_secs(query).map(|secs| secs.to_string()),
        });
    Ok(send_json::<Track>(request)
        .await?
        .and_then(|track| track_lyrics(&track)))
}

async fn search_tracks(
    client: &reqwest::Client,
    base: &str,
    query: &LyricsQuery,
) -> Result<Vec<Track>> {
    let request = client
        .get(format!("{base}/api/search"))
        .header(USER_AGENT, AGENT)
        .timeout(TIMEOUT)
        .query(&[("q", format!("{} {}", query.title, query.artist))]);
    Ok(send_json(request).await?.unwrap_or_default())
}

fn best_search_match(
    tracks: &[Track],
    query: &LyricsQuery,
    mut fallback: Option<Lyrics>,
) -> Option<Lyrics> {
    let wanted_duration = duration_secs(query);
    let items: Vec<SearchItem<'_>> = tracks.iter().map(search_item).collect();
    for track in close_matches(query, &items, tracks) {
        if !duration_matches(track.duration, wanted_duration) {
            continue;
        }
        let Some(lyrics) = track_lyrics(track) else {
            continue;
        };
        if lyrics.is_synced {
            return Some(lyrics);
        }
        fallback.get_or_insert(lyrics);
    }
    fallback
}

fn duration_secs(query: &LyricsQuery) -> Option<f64> {
    query.duration_ms.map(ms_to_seconds)
}

fn search_item(track: &Track) -> SearchItem<'_> {
    SearchItem {
        name: &track.name,
        artist: &track.artist_name,
        is_sync: Some(
            track
                .synced_lyrics
                .as_deref()
                .is_some_and(|synced| !synced.is_empty()),
        ),
    }
}

fn duration_matches(candidate: Option<f64>, wanted: Option<f64>) -> bool {
    match (candidate, wanted) {
        (Some(candidate), Some(wanted)) => {
            (candidate - wanted).abs() <= SERVER_SIDE_DURATION_TOLERANCE_SECS
        }
        _ => true,
    }
}

fn track_lyrics(track: &Track) -> Option<Lyrics> {
    [&track.synced_lyrics, &track.plain_lyrics]
        .into_iter()
        .flatten()
        .filter(|text| !text.trim().is_empty())
        .find_map(|text| lyrics_from_text(LyricsSource::Lrclib, text))
}

#[cfg(test)]
mod tests;
