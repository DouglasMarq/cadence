use cadence_core::queue::{QueueTrack, RepeatMode};
use cadence_storage::cover_cache::DecodedImage;
use slint::{ComponentHandle, Image};

use super::images::{CoverSource, tint_of, to_image};
use super::{
    AppWindow, Context, PlayerState, RepeatState, clamp_i32, on_ui, queue, radio, show_status,
    tracks,
};
use crate::controller::{Command, MAX_VOLUME, UiSink, UiUpdate};

const COVER_SIZE: u32 = 128;

pub fn install(window: &AppWindow, context: &Context) {
    let state = window.global::<PlayerState>();
    let bind = |command: fn() -> Command| {
        let context = context.clone();
        move || context.send(command())
    };
    state.on_toggle_pause(bind(|| Command::TogglePause));
    state.on_next(bind(|| Command::Next));
    state.on_previous(bind(|| Command::Previous));
    state.on_toggle_mute(bind(|| Command::ToggleMute));
    state.on_toggle_shuffle(bind(|| Command::ToggleShuffle));
    state.on_cycle_repeat(bind(|| Command::CycleRepeat));
    let seek_context = context.clone();
    state.on_seek(move |position_ms| {
        seek_context.send(Command::Seek(u64::try_from(position_ms).unwrap_or(0)));
    });
    let volume_context = context.clone();
    state.on_set_volume(move |volume| {
        volume_context.send(Command::SetVolume(
            u8::try_from(volume.clamp(0, i32::from(MAX_VOLUME))).unwrap_or_default(),
        ));
    });
}

pub struct SlintSink {
    window: slint::Weak<AppWindow>,
    context: Context,
}

impl SlintSink {
    pub fn new(window: slint::Weak<AppWindow>, context: Context) -> Self {
        Self { window, context }
    }

    fn load_cover(&self, track: &QueueTrack) {
        let Some(source) = CoverSource::of_track(track, self.context.session.get()) else {
            return;
        };
        let song_id = track.song_id.to_string();
        let covers = self.context.covers.clone();
        let window = self.window.clone();
        self.context.runtime.spawn(async move {
            if let Some(cover) = source.fetch(covers.as_ref(), COVER_SIZE).await {
                on_ui(&window, move |window| show_cover(window, &song_id, &cover));
            }
        });
    }
}

fn show_cover(window: &AppWindow, song_id: &str, cover: &DecodedImage) {
    let state = window.global::<PlayerState>();
    if state.get_song_id() != song_id {
        return;
    }
    if let Some(image) = to_image(cover) {
        state.set_cover(image);
        state.set_has_cover(true);
    }
    let tint = tint_of(cover);
    state.set_has_tint(tint.is_some());
    if let Some(tint) = tint {
        state.set_tint(tint);
    }
}

impl UiSink for SlintSink {
    fn send(&self, update: UiUpdate) {
        let cover_for = match &update {
            UiUpdate::NowPlaying(Some(track)) => Some(track.clone()),
            _ => None,
        };
        on_ui(&self.window, move |window| apply(window, update));
        if let Some(track) = cover_for {
            self.load_cover(&track);
        }
    }
}

fn show_song(window: &AppWindow, track: &QueueTrack) {
    let state = window.global::<PlayerState>();
    let song_id = track.song_id.as_str();
    if state.get_song_id() != song_id {
        state.set_has_cover(false);
        state.set_cover(Image::default());
        state.set_has_tint(false);
    }
    state.set_has_song(true);
    state.set_song_id(song_id.into());
    state.set_title((*track.title).into());
    state.set_artists(track.artist_names().into());
    state.set_album(track.album.as_deref().unwrap_or_default().into());
    tracks::set_current(window, Some(song_id));
}

fn clear_song(window: &AppWindow) {
    let state = window.global::<PlayerState>();
    state.set_has_song(false);
    state.set_has_cover(false);
    state.set_has_tint(false);
    state.set_cover(Image::default());
    state.set_song_id("".into());
    state.set_title("".into());
    state.set_artists("".into());
    state.set_album("".into());
    tracks::set_current(window, None);
}

fn repeat_state(repeat: RepeatMode) -> RepeatState {
    match repeat {
        RepeatMode::None => RepeatState::Off,
        RepeatMode::All => RepeatState::All,
        RepeatMode::One => RepeatState::One,
    }
}

fn raise(window: &AppWindow) {
    if let Err(error) = window.show() {
        tracing::debug!("raise: {error}");
    }
}

fn quit() {
    if let Err(error) = slint::quit_event_loop() {
        tracing::debug!("quit: {error}");
    }
}

fn apply(window: &AppWindow, update: UiUpdate) {
    let state = window.global::<PlayerState>();
    match update {
        UiUpdate::NowPlaying(Some(track)) => show_song(window, &track),
        UiUpdate::NowPlaying(None) => clear_song(window),
        UiUpdate::Playback {
            is_playing,
            position_ms,
            duration_ms,
        } => {
            state.set_is_playing(is_playing);
            state.set_position_ms(clamp_i32(position_ms));
            state.set_duration_ms(clamp_i32(duration_ms));
        }
        UiUpdate::Volume { volume, is_muted } => {
            state.set_volume(i32::from(volume));
            state.set_is_muted(is_muted);
        }
        UiUpdate::Modes {
            is_shuffled,
            repeat,
        } => {
            state.set_is_shuffled(is_shuffled);
            state.set_repeat(repeat_state(repeat));
        }
        UiUpdate::Queue(view) => queue::set_view(window, view),
        UiUpdate::Radio(now_playing) => radio::show_now_playing(window, now_playing.as_ref()),
        UiUpdate::Error(message) => show_status(window, &message),
        UiUpdate::Raise => raise(window),
        UiUpdate::Quit => quit(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cadence_core::queue::Queue;
    use slint::Model;

    use super::*;
    use crate::controller::RadioNowPlaying;
    use crate::demo::{FakeServer, test_song};
    use crate::ui::test_window::TestWindow;
    use crate::ui::{QueueState, RadioState};

    fn open_player() -> Option<TestWindow> {
        TestWindow::open(Arc::new(FakeServer::new(30)))
    }

    fn sink_of(view: &TestWindow) -> SlintSink {
        SlintSink::new(view.app.as_weak(), view.context.clone())
    }

    fn sent_commands(view: &TestWindow) -> Vec<Command> {
        std::iter::from_fn(|| view.next_command()).collect()
    }

    fn track_with_cover(song_id: &str) -> Box<QueueTrack> {
        let song = cadence_core::model::Song {
            cover_art_id: Some(format!("cover-{song_id}")),
            album: Some("Night Drive".into()),
            ..test_song(song_id, 180_000)
        };
        Box::new(QueueTrack::from(&song))
    }

    #[test]
    fn given_the_player_controls_when_pressed_then_they_send_their_commands() {
        let Some(view) = open_player() else {
            return;
        };
        let state = view.app.global::<PlayerState>();
        sent_commands(&view);

        state.invoke_toggle_pause();
        state.invoke_next();
        state.invoke_previous();
        state.invoke_toggle_mute();
        state.invoke_toggle_shuffle();
        state.invoke_cycle_repeat();
        state.invoke_seek(-5);
        state.invoke_set_volume(250);

        let commands = sent_commands(&view);
        assert!(matches!(
            commands.as_slice(),
            [
                Command::TogglePause,
                Command::Next,
                Command::Previous,
                Command::ToggleMute,
                Command::ToggleShuffle,
                Command::CycleRepeat,
                Command::Seek(0),
                Command::SetVolume(MAX_VOLUME),
            ]
        ));
    }

    #[test]
    fn given_a_new_song_when_it_starts_then_the_player_bar_shows_it_with_its_cover() {
        let Some(view) = open_player() else {
            return;
        };
        let state = view.app.global::<PlayerState>();

        sink_of(&view).send(UiUpdate::NowPlaying(Some(track_with_cover("song-3"))));

        assert!(view.pump(|| state.get_has_cover()));
        assert!(state.get_has_song());
        assert_eq!(state.get_song_id(), "song-3");
        assert_eq!(state.get_title(), "song-3");
        assert_eq!(state.get_album(), "Night Drive");
    }

    #[test]
    fn given_a_song_change_when_the_old_cover_arrives_late_then_the_new_song_keeps_the_bar() {
        let Some(view) = open_player() else {
            return;
        };
        let state = view.app.global::<PlayerState>();
        let sink = sink_of(&view);

        sink.send(UiUpdate::NowPlaying(Some(track_with_cover("song-3"))));
        sink.send(UiUpdate::NowPlaying(Some(track_with_cover("song-4"))));
        sink.send(UiUpdate::NowPlaying(Some(track_with_cover("song-4"))));

        assert!(view.pump(|| state.get_has_cover()));
        assert_eq!(state.get_song_id(), "song-4");
    }

    #[test]
    fn given_playback_stops_when_nothing_plays_then_the_player_bar_is_cleared() {
        let Some(view) = open_player() else {
            return;
        };
        let state = view.app.global::<PlayerState>();
        let sink = sink_of(&view);

        sink.send(UiUpdate::NowPlaying(Some(track_with_cover("song-3"))));
        assert!(view.pump(|| state.get_has_cover()));
        sink.send(UiUpdate::NowPlaying(None));

        assert!(view.pump(|| !state.get_has_song()));
        assert!(!state.get_has_cover());
        assert_eq!(
            (state.get_song_id(), state.get_title()),
            ("".into(), "".into())
        );
    }

    #[test]
    fn given_playback_volume_and_mode_updates_when_sent_then_the_player_bar_follows_them() {
        let Some(view) = open_player() else {
            return;
        };
        let state = view.app.global::<PlayerState>();
        let sink = sink_of(&view);

        sink.send(UiUpdate::Playback {
            is_playing: true,
            position_ms: 1_500,
            duration_ms: 90_000,
        });
        sink.send(UiUpdate::Volume {
            volume: 42,
            is_muted: true,
        });
        sink.send(UiUpdate::Modes {
            is_shuffled: true,
            repeat: RepeatMode::One,
        });

        assert!(view.pump(|| state.get_repeat() == RepeatState::One));
        assert!(state.get_is_playing());
        assert_eq!(
            (state.get_position_ms(), state.get_duration_ms()),
            (1_500, 90_000)
        );
        assert_eq!((state.get_volume(), state.get_is_muted()), (42, true));
        assert!(state.get_is_shuffled());
    }

    #[test]
    fn given_queue_radio_and_error_updates_when_sent_then_the_window_shows_them() {
        let Some(view) = open_player() else {
            return;
        };
        let sink = sink_of(&view);
        let mut queue = Queue::new();
        queue.add_last(vec![test_song("song-1", 1_000), test_song("song-2", 1_000)]);

        sink.send(UiUpdate::Queue(queue.view()));
        sink.send(UiUpdate::Radio(Some(RadioNowPlaying {
            station_id: "station-1".into(),
            station_name: "Harbour FM".into(),
            stream_title: None,
        })));
        sink.send(UiUpdate::Error("the speakers caught fire".into()));
        sink.send(UiUpdate::Raise);
        sink.send(UiUpdate::Quit);

        assert!(view.pump(|| view.status() == "the speakers caught fire"));
        let rows = view.app.global::<QueueState>().get_rows();
        assert_eq!(rows.row_count(), 2);
        assert_eq!(
            view.app.global::<RadioState>().get_playing_id(),
            "station-1"
        );
    }

    #[test]
    fn given_each_repeat_mode_the_player_bar_shows_the_matching_state() {
        assert_eq!(repeat_state(RepeatMode::None), RepeatState::Off);
        assert_eq!(repeat_state(RepeatMode::All), RepeatState::All);
        assert_eq!(repeat_state(RepeatMode::One), RepeatState::One);
    }
}
