use std::sync::Arc;

use cadence_remote::HostState;
use tokio::sync::watch;

use crate::controller::{UiSink, UiUpdate};
use crate::sleep_timer::SleepStatus;

pub struct HostFeed<Inner> {
    inner: Inner,
    state: watch::Sender<HostState>,
}

impl<Inner> HostFeed<Inner> {
    pub fn new(inner: Inner, state: watch::Sender<HostState>) -> Self {
        Self { inner, state }
    }
}

fn record(state: &mut HostState, update: &UiUpdate) {
    match update {
        UiUpdate::NowPlaying(track) => {
            state.now_playing = track.as_deref().cloned().map(Arc::new);
        }
        UiUpdate::Playback {
            is_playing,
            position_ms,
            duration_ms,
        } => {
            state.is_playing = *is_playing;
            state.position_ms = *position_ms;
            state.duration_ms = *duration_ms;
        }
        UiUpdate::Volume { volume, is_muted } => {
            state.volume = *volume;
            state.is_muted = *is_muted;
        }
        UiUpdate::Modes {
            is_shuffled,
            repeat,
        } => {
            state.is_shuffled = *is_shuffled;
            state.repeat = *repeat;
        }
        UiUpdate::Queue(view) => state.queue = view.clone(),
        UiUpdate::Radio(_) | UiUpdate::Error(_) | UiUpdate::Raise | UiUpdate::Quit => {}
    }
}

fn is_recorded(update: &UiUpdate) -> bool {
    !matches!(
        update,
        UiUpdate::Radio(_) | UiUpdate::Error(_) | UiUpdate::Raise | UiUpdate::Quit
    )
}

impl<Inner: UiSink> UiSink for HostFeed<Inner> {
    fn send(&self, update: UiUpdate) {
        if is_recorded(&update) {
            self.state.send_modify(|state| record(state, &update));
        }
        self.inner.send(update);
    }

    fn sleep_timer(&self, status: Option<SleepStatus>) {
        self.inner.sleep_timer(status);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use cadence_core::queue::{Queue, QueueTrack, RepeatMode, YouTubeTrack};

    use super::*;
    use crate::remote::gate::LocalGate;
    use crate::remote::target::{PlayerTarget, RemoteTarget};

    #[derive(Default)]
    struct Recorded {
        updates: Mutex<Vec<UiUpdate>>,
        sleep_timers: Mutex<Vec<Option<SleepStatus>>>,
    }

    impl UiSink for Arc<Recorded> {
        fn send(&self, update: UiUpdate) {
            self.updates.lock().unwrap().push(update);
        }

        fn sleep_timer(&self, status: Option<SleepStatus>) {
            self.sleep_timers.lock().unwrap().push(status);
        }
    }

    fn track() -> QueueTrack {
        QueueTrack::from(&YouTubeTrack {
            video_id: "v1".into(),
            title: "Song".into(),
            artists: Vec::new(),
            album: None,
            thumbnail_url: None,
            duration_ms: 90_000,
        })
    }

    #[test]
    fn given_the_song_position_and_queue_when_shown_then_the_host_state_follows_them() {
        let shown = Arc::new(Recorded::default());
        let (state, watched) = watch::channel(HostState::default());
        let sink = HostFeed::new(Arc::clone(&shown), state);
        let mut queue = Queue::with_seed(3);
        queue.play_tracks_now(vec![track()], Some(0));

        sink.send(UiUpdate::NowPlaying(Some(Box::new(track()))));
        sink.send(UiUpdate::Playback {
            is_playing: true,
            position_ms: 4_000,
            duration_ms: 90_000,
        });
        sink.send(UiUpdate::Queue(queue.view()));
        sink.send(UiUpdate::Raise);

        let host = watched.borrow();
        assert_eq!(host.now_playing.as_deref(), Some(&track()));
        assert_eq!(
            (host.is_playing, host.position_ms, host.duration_ms),
            (true, 4_000, 90_000)
        );
        assert_eq!(host.queue.len(), 1);
        assert_eq!(shown.updates.lock().unwrap().len(), 4);
    }

    #[test]
    fn given_a_sleep_timer_when_shown_then_it_reaches_the_screen_through_both_sinks() {
        let shown = Arc::new(Recorded::default());
        let (state, _watched) = watch::channel(HostState::default());
        let sink = HostFeed::new(
            LocalGate::new(Arc::clone(&shown), PlayerTarget::default()),
            state,
        );

        sink.sleep_timer(Some(SleepStatus::EndOfTrack));
        sink.sleep_timer(None);

        assert_eq!(
            *shown.sleep_timers.lock().unwrap(),
            [Some(SleepStatus::EndOfTrack), None]
        );
    }

    #[test]
    fn this_player_keeps_publishing_its_state_while_the_screen_shows_another() {
        let shown = Arc::new(Recorded::default());
        let target = PlayerTarget::default();
        let (state, watched) = watch::channel(HostState::default());
        let sink = HostFeed::new(LocalGate::new(Arc::clone(&shown), target.clone()), state);
        sink.send(UiUpdate::Volume {
            volume: 20,
            is_muted: false,
        });
        let (messages, _sent) = tokio::sync::mpsc::unbounded_channel();
        target.set(Some(RemoteTarget { messages }));
        sink.send(UiUpdate::Modes {
            is_shuffled: true,
            repeat: RepeatMode::One,
        });
        sink.send(UiUpdate::Error("boom".into()));
        assert_eq!(
            shown.updates.lock().unwrap().len(),
            2,
            "volume and the error"
        );
        let host = watched.borrow();
        assert_eq!(
            (host.volume, host.is_shuffled, host.repeat),
            (20, true, RepeatMode::One)
        );
    }
}
