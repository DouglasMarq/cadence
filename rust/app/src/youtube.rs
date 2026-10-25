use cadence_core::queue::{QueueTrack, YouTubeTrack};
use cadence_storage::settings::YouTubeSettings;
use cadence_youtube::music::MusicSong;
use cadence_youtube::{YouTubeHit, YtDlp, thumbnail_url};

pub fn yt_dlp_for(settings: &YouTubeSettings) -> YtDlp {
    YtDlp::new(settings.yt_dlp_program()).with_cookies_from(settings.cookies_from_browser)
}

pub fn youtube_track_of(song: &MusicSong) -> YouTubeTrack {
    YouTubeTrack {
        video_id: song.video_id.clone(),
        title: song.title.clone(),
        artists: song.artists.clone(),
        album: song.album.clone(),
        thumbnail_url: Some(
            song.thumbnail_url
                .clone()
                .unwrap_or_else(|| thumbnail_url(&song.video_id)),
        ),
        duration_ms: song.duration_ms.unwrap_or_default(),
    }
}

fn music_song_of(hit: &YouTubeHit) -> MusicSong {
    MusicSong {
        video_id: hit.video_id.clone(),
        title: hit.title.clone(),
        artists: hit.artist.iter().cloned().collect(),
        album: hit.album.clone(),
        duration_ms: hit.duration_ms,
        thumbnail_url: hit.thumbnail_url.clone(),
    }
}

pub fn queue_track_of(hit: &YouTubeHit) -> QueueTrack {
    QueueTrack::from(&youtube_track_of(&music_song_of(hit)))
}

#[cfg(test)]
mod tests {
    use cadence_youtube::CookieBrowser;

    use super::*;

    fn hit() -> YouTubeHit {
        YouTubeHit {
            video_id: "abc".into(),
            title: "Song".into(),
            artist: Some("Singer".into()),
            album: None,
            thumbnail_url: None,
            duration_ms: Some(90_000),
        }
    }

    #[test]
    fn hits_become_queue_tracks_with_a_thumbnail() {
        let track = queue_track_of(&hit());
        assert_eq!(track.song_id, "yt:abc");
        assert_eq!(track.artist_names(), "Singer");
        assert_eq!(track.duration_ms, 90_000);
        assert_eq!(
            track.thumbnail_url(),
            Some("https://i.ytimg.com/vi/abc/hqdefault.jpg")
        );
    }

    #[test]
    fn yt_dlp_follows_the_settings() {
        let settings = YouTubeSettings {
            yt_dlp_path: Some("/opt/yt-dlp".into()),
            cookies_from_browser: Some(CookieBrowser::Brave),
            country: None,
        };
        assert_eq!(
            yt_dlp_for(&settings),
            YtDlp::new("/opt/yt-dlp").with_cookies_from(Some(CookieBrowser::Brave))
        );
    }
}
