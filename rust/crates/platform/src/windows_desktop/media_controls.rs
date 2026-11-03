use std::path::Path;
use std::sync::Arc;

use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::StorageFile;
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::core::{HSTRING, Ref};

use crate::cover_art::{CoverArt, cover_art};
use crate::{MediaControlEvent, MediaIdentity, NowPlaying, PlatformError, PlaybackStatus, Result};

const TICKS_PER_MILLISECOND: i64 = 10_000;
const ARTIST_SEPARATOR: &str = ", ";

type EventHandler = Arc<dyn Fn(MediaControlEvent) + Send + Sync>;

pub struct MediaControls {
    _player: MediaPlayer,
    controls: SystemMediaTransportControls,
    status: PlaybackStatus,
    duration_ms: u64,
}

impl MediaControls {
    pub fn new(
        _shown_by_the_executable_details: MediaIdentity<'_>,
        on_event: Box<dyn Fn(MediaControlEvent) + Send + Sync>,
    ) -> Result<Self> {
        let player = MediaPlayer::new().map_err(media_error)?;
        player
            .CommandManager()
            .and_then(|manager| manager.SetIsEnabled(false))
            .map_err(media_error)?;
        let controls = player.SystemMediaTransportControls().map_err(media_error)?;
        enable_buttons(&controls).map_err(media_error)?;
        listen(&controls, Arc::from(on_event)).map_err(media_error)?;
        Ok(Self {
            _player: player,
            controls,
            status: PlaybackStatus::Stopped,
            duration_ms: 0,
        })
    }

    pub fn set_metadata(&mut self, now_playing: &NowPlaying) -> Result<()> {
        self.duration_ms = now_playing.duration_ms.unwrap_or_default();
        show_metadata(&self.controls, now_playing).map_err(media_error)?;
        self.publish_status()
    }

    pub fn set_status(&mut self, status: PlaybackStatus) -> Result<()> {
        self.status = status;
        self.publish_status()
    }

    pub fn set_volume(&mut self, _volume: f64) -> Result<()> {
        Ok(())
    }

    pub fn set_live_stream(&mut self, _is_live: bool) -> Result<()> {
        Ok(())
    }

    fn publish_status(&self) -> Result<()> {
        self.controls
            .SetPlaybackStatus(playback_status_of(self.status))
            .and_then(|()| timeline_of(self.status, self.duration_ms))
            .and_then(|timeline| self.controls.UpdateTimelineProperties(&timeline))
            .map_err(media_error)
    }
}

fn enable_buttons(controls: &SystemMediaTransportControls) -> windows::core::Result<()> {
    controls.SetIsEnabled(true)?;
    controls.SetIsPlayEnabled(true)?;
    controls.SetIsPauseEnabled(true)?;
    controls.SetIsStopEnabled(true)?;
    controls.SetIsNextEnabled(true)?;
    controls.SetIsPreviousEnabled(true)
}

fn listen(
    controls: &SystemMediaTransportControls,
    on_event: EventHandler,
) -> windows::core::Result<()> {
    let on_button = Arc::clone(&on_event);
    controls.ButtonPressed(&TypedEventHandler::new(
        move |_controls, pressed: Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
            if let Some(event) = event_of_button(pressed.ok()?.Button()?) {
                on_button(event);
            }
            Ok(())
        },
    ))?;
    controls.PlaybackPositionChangeRequested(&TypedEventHandler::new(
        move |_controls, requested: Ref<PlaybackPositionChangeRequestedEventArgs>| {
            on_event(seek_to(requested.ok()?.RequestedPlaybackPosition()?));
            Ok(())
        },
    ))?;
    Ok(())
}

fn show_metadata(
    controls: &SystemMediaTransportControls,
    now_playing: &NowPlaying,
) -> windows::core::Result<()> {
    let updater = controls.DisplayUpdater()?;
    updater.ClearAll()?;
    updater.SetType(MediaPlaybackType::Music)?;
    let music = updater.MusicProperties()?;
    music.SetTitle(&HSTRING::from(now_playing.title.as_str()))?;
    music.SetArtist(&HSTRING::from(now_playing.artists.join(ARTIST_SEPARATOR)))?;
    music.SetAlbumTitle(&HSTRING::from(
        now_playing.album.as_deref().unwrap_or_default(),
    ))?;
    if let Some(thumbnail) = thumbnail_of(now_playing) {
        updater.SetThumbnail(&thumbnail)?;
    }
    updater.Update()
}

fn thumbnail_of(now_playing: &NowPlaying) -> Option<RandomAccessStreamReference> {
    let thumbnail = match cover_art(now_playing)? {
        CoverArt::Url(url) => Uri::CreateUri(&HSTRING::from(url))
            .and_then(|uri| RandomAccessStreamReference::CreateFromUri(&uri)),
        CoverArt::File(cover_file) => file_thumbnail(cover_file),
    };
    thumbnail
        .inspect_err(|error| tracing::debug!("the media cover is unavailable: {error}"))
        .ok()
}

fn file_thumbnail(cover_file: &Path) -> windows::core::Result<RandomAccessStreamReference> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(cover_file))?.join()?;
    RandomAccessStreamReference::CreateFromFile(&file)
}

fn event_of_button(button: SystemMediaTransportControlsButton) -> Option<MediaControlEvent> {
    match button {
        SystemMediaTransportControlsButton::Play => Some(MediaControlEvent::Play),
        SystemMediaTransportControlsButton::Pause => Some(MediaControlEvent::Pause),
        SystemMediaTransportControlsButton::Stop => Some(MediaControlEvent::Stop),
        SystemMediaTransportControlsButton::Next => Some(MediaControlEvent::Next),
        SystemMediaTransportControlsButton::Previous => Some(MediaControlEvent::Previous),
        _ => None,
    }
}

fn seek_to(position: TimeSpan) -> MediaControlEvent {
    MediaControlEvent::SeekTo {
        position_ms: u64::try_from(position.Duration / TICKS_PER_MILLISECOND).unwrap_or_default(),
    }
}

fn playback_status_of(status: PlaybackStatus) -> MediaPlaybackStatus {
    match status {
        PlaybackStatus::Playing { .. } => MediaPlaybackStatus::Playing,
        PlaybackStatus::Paused { .. } => MediaPlaybackStatus::Paused,
        PlaybackStatus::Stopped => MediaPlaybackStatus::Stopped,
    }
}

fn timeline_of(
    status: PlaybackStatus,
    duration_ms: u64,
) -> windows::core::Result<SystemMediaTransportControlsTimelineProperties> {
    let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
    timeline.SetStartTime(time_span(0))?;
    timeline.SetMinSeekTime(time_span(0))?;
    timeline.SetEndTime(time_span(duration_ms))?;
    timeline.SetMaxSeekTime(time_span(duration_ms))?;
    timeline.SetPosition(time_span(position_of(status)))?;
    Ok(timeline)
}

fn position_of(status: PlaybackStatus) -> u64 {
    match status {
        PlaybackStatus::Playing { position_ms } | PlaybackStatus::Paused { position_ms } => {
            position_ms
        }
        PlaybackStatus::Stopped => 0,
    }
}

fn time_span(milliseconds: u64) -> TimeSpan {
    TimeSpan {
        Duration: i64::try_from(milliseconds)
            .unwrap_or(i64::MAX)
            .saturating_mul(TICKS_PER_MILLISECOND),
    }
}

fn media_error(error: windows::core::Error) -> PlatformError {
    PlatformError::MediaControls(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: MediaIdentity<'static> = MediaIdentity {
        name: "Cadence",
        desktop_entry: "io.github.douglasmarq.cadence",
    };

    #[test]
    fn given_each_enabled_button_when_pressed_then_it_is_the_same_player_event() {
        let expected = [
            (
                SystemMediaTransportControlsButton::Play,
                MediaControlEvent::Play,
            ),
            (
                SystemMediaTransportControlsButton::Pause,
                MediaControlEvent::Pause,
            ),
            (
                SystemMediaTransportControlsButton::Stop,
                MediaControlEvent::Stop,
            ),
            (
                SystemMediaTransportControlsButton::Next,
                MediaControlEvent::Next,
            ),
            (
                SystemMediaTransportControlsButton::Previous,
                MediaControlEvent::Previous,
            ),
        ];
        for (button, event) in expected {
            assert_eq!(event_of_button(button), Some(event));
        }
        assert_eq!(
            event_of_button(SystemMediaTransportControlsButton::ChannelUp),
            None
        );
    }

    #[test]
    fn given_a_requested_position_when_seeking_then_it_is_converted_to_milliseconds() {
        assert_eq!(
            seek_to(time_span(61_500)),
            MediaControlEvent::SeekTo {
                position_ms: 61_500
            }
        );
        assert_eq!(
            seek_to(TimeSpan { Duration: -1 }),
            MediaControlEvent::SeekTo { position_ms: 0 }
        );
    }

    #[test]
    fn given_a_status_when_published_then_its_position_and_state_are_shown() {
        assert_eq!(
            position_of(PlaybackStatus::Playing { position_ms: 3_400 }),
            3_400
        );
        assert_eq!(position_of(PlaybackStatus::Stopped), 0);
        assert_eq!(
            playback_status_of(PlaybackStatus::Paused { position_ms: 1 }),
            MediaPlaybackStatus::Paused
        );
    }

    #[test]
    fn given_this_computer_when_publishing_a_song_then_the_system_controls_accept_it() {
        let mut controls = MediaControls::new(IDENTITY, Box::new(|_event| {})).unwrap();
        controls
            .set_metadata(&NowPlaying {
                track_id: "song-1".to_owned(),
                title: "Song".to_owned(),
                artists: vec!["Artist".to_owned()],
                album: Some("Album".to_owned()),
                duration_ms: Some(215_500),
                cover_url: Some("https://example.com/cover.jpg".to_owned()),
                cover_file: None,
            })
            .unwrap();
        controls
            .set_status(PlaybackStatus::Playing { position_ms: 1_000 })
            .unwrap();
        controls.set_status(PlaybackStatus::Stopped).unwrap();
    }
}
