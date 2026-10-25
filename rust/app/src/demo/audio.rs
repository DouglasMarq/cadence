use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::Result;
use cadence_player::{AudioDevice, PlayerEvent};

use crate::audio::{AudioOutput, EventCallback, Track};

const DEMO_STREAM_TITLE: &str = "Demo Artist - Live on the demo radio";
const LONGEST_OVERLAP_PERCENT: u64 = 40;

#[derive(Default)]
struct FakeState {
    current: Option<Track>,
    next: Option<Track>,
    position_ms: u64,
    is_playing: bool,
    crossfade_ms: u64,
    events: Vec<PlayerEvent>,
}

impl FakeState {
    fn emit(&mut self, event: PlayerEvent) {
        self.events.push(event);
    }

    fn take_events(&mut self) -> Vec<PlayerEvent> {
        std::mem::take(&mut self.events)
    }
}

pub struct FakeAudio {
    state: Arc<Mutex<FakeState>>,
    on_event: Arc<EventCallback>,
}

impl FakeAudio {
    pub const TICK: Duration = Duration::from_millis(250);

    pub fn new(on_event: EventCallback) -> Self {
        let state = Arc::new(Mutex::new(FakeState::default()));
        let on_event = Arc::new(on_event);
        let clock_state = Arc::downgrade(&state);
        let clock_events = Arc::clone(&on_event);
        if let Err(error) = std::thread::Builder::new()
            .name("fake-audio".into())
            .spawn(move || Self::clock(clock_state, clock_events))
        {
            tracing::warn!("starting the demo audio clock failed: {error}");
        }
        Self { state, on_event }
    }

    pub fn with_crossfade(self, crossfade: Duration) -> Self {
        if let Err(error) = self.set_crossfade(crossfade) {
            tracing::warn!("setting the demo crossfade failed: {error:#}");
        }
        self
    }

    fn clock(state: Weak<Mutex<FakeState>>, on_event: Arc<EventCallback>) {
        while let Some(shared_state) = state.upgrade() {
            std::thread::sleep(Self::TICK);
            let events = match shared_state.lock() {
                Ok(mut locked_state) => {
                    Self::tick(&mut locked_state, Self::TICK.as_millis() as u64);
                    locked_state.take_events()
                }
                Err(_) => return,
            };
            drop(shared_state);
            events.into_iter().for_each(|event| on_event(event));
        }
    }

    fn tick(state: &mut FakeState, elapsed_ms: u64) {
        let Some(current) = state.current.as_ref().filter(|_| state.is_playing) else {
            return;
        };
        if current.is_live_stream() {
            state.position_ms += elapsed_ms;
            state.emit(PlayerEvent::Position {
                position_ms: state.position_ms,
                duration_ms: None,
            });
            return;
        }
        let duration_ms = current.duration_ms;
        state.position_ms = (state.position_ms + elapsed_ms).min(duration_ms);
        state.emit(PlayerEvent::Position {
            position_ms: state.position_ms,
            duration_ms: Some(duration_ms),
        });
        Self::advance_when_due(state, duration_ms);
    }

    fn advance_when_due(state: &mut FakeState, duration_ms: u64) {
        let overlap_ms = match state.next {
            Some(_) => state
                .crossfade_ms
                .min(duration_ms * LONGEST_OVERLAP_PERCENT / 100),
            None => 0,
        };
        if state.position_ms + overlap_ms < duration_ms {
            return;
        }
        let ended = state.current.as_ref().filter(|_| overlap_ms == 0);
        if let Some(track) = ended {
            let uid = track.uid;
            state.emit(PlayerEvent::TrackEnded { uid });
        }
        state.position_ms = 0;
        state.current = state.next.take();
        match state.current.as_ref().map(|next| next.uid) {
            Some(uid) => state.emit(PlayerEvent::Started { uid }),
            None => {
                state.is_playing = false;
                state.emit(PlayerEvent::Idle);
            }
        }
    }

    fn update(&self, mutation: impl FnOnce(&mut FakeState)) -> Result<()> {
        let events = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("fake audio state poisoned"))?;
            mutation(&mut state);
            state.take_events()
        };
        events.into_iter().for_each(|event| (self.on_event)(event));
        Ok(())
    }
}

impl AudioOutput for FakeAudio {
    fn load(&self, track: &Track, start_ms: u64) -> Result<()> {
        self.update(|state| {
            state.current = Some(track.clone());
            state.next = None;
            state.position_ms = start_ms;
            state.is_playing = true;
            state.emit(PlayerEvent::Started { uid: track.uid });
        })?;
        if track.is_live_stream() {
            (self.on_event)(PlayerEvent::StreamTitle(Some(DEMO_STREAM_TITLE.into())));
        }
        Ok(())
    }

    fn preload_next(&self, track: &Track) -> Result<()> {
        self.update(|state| {
            state.next = Some(track.clone());
        })
    }

    fn clear_next(&self) -> Result<()> {
        self.update(|state| {
            state.next = None;
        })
    }

    fn play(&self) -> Result<()> {
        self.update(|state| {
            let has_resumed = state.current.is_some() && !state.is_playing;
            state.is_playing |= state.current.is_some();
            if has_resumed {
                state.emit(PlayerEvent::Resumed);
            }
        })
    }

    fn pause(&self) -> Result<()> {
        self.update(|state| {
            let was_playing = state.is_playing;
            state.is_playing = false;
            if was_playing {
                state.emit(PlayerEvent::Paused);
            }
        })
    }

    fn stop(&self) -> Result<()> {
        self.update(|state| {
            *state = FakeState {
                crossfade_ms: state.crossfade_ms,
                ..FakeState::default()
            };
        })
    }

    fn seek(&self, position_ms: u64) -> Result<()> {
        self.update(|state| {
            let Some(duration_ms) = state.current.as_ref().map(|current| current.duration_ms)
            else {
                return;
            };
            state.position_ms = position_ms.min(duration_ms);
            state.emit(PlayerEvent::Position {
                position_ms: state.position_ms,
                duration_ms: Some(duration_ms),
            });
        })
    }

    fn set_volume(&self, _volume: u8) -> Result<()> {
        Ok(())
    }

    fn set_muted(&self, _is_muted: bool) -> Result<()> {
        Ok(())
    }

    fn set_crossfade(&self, crossfade: Duration) -> Result<()> {
        self.update(|state| {
            state.crossfade_ms = u64::try_from(crossfade.as_millis()).unwrap_or(u64::MAX);
        })
    }

    fn audio_devices(&self) -> Result<Vec<AudioDevice>> {
        let device = |name: &str, description: &str| AudioDevice {
            name: name.into(),
            description: description.into(),
        };
        Ok(vec![
            device("auto", "Autoselect device"),
            device("pipewire/demo-speakers", "Demo speakers"),
            device("pipewire/demo-headphones", "Demo headphones"),
        ])
    }
}

#[cfg(test)]
mod tests {
    use cadence_core::queue::Uid;

    use super::*;

    fn tick(state: &mut FakeState, elapsed_ms: u64) -> Vec<PlayerEvent> {
        FakeAudio::tick(state, elapsed_ms);
        state.take_events()
    }

    fn track(uid: Uid, duration_ms: u64) -> Track {
        Track {
            uid,
            url: String::new(),
            duration_ms,
        }
    }

    #[test]
    fn fake_clock_ends_track_and_takes_over_preload() {
        let mut state = FakeState {
            current: Some(track(1, 500)),
            next: Some(track(2, 500)),
            is_playing: true,
            ..FakeState::default()
        };
        assert_eq!(tick(&mut state, 250).len(), 1);
        let events = tick(&mut state, 250);
        assert_eq!(events[1], PlayerEvent::TrackEnded { uid: 1 });
        assert_eq!(events[2], PlayerEvent::Started { uid: 2 });
        tick(&mut state, 500);
        assert_eq!(tick(&mut state, 0), Vec::new());
    }

    #[test]
    fn fake_clock_crossfades_into_the_preload_before_the_end() {
        let mut state = FakeState {
            current: Some(track(1, 10_000)),
            next: Some(track(2, 10_000)),
            is_playing: true,
            crossfade_ms: 3_000,
            ..FakeState::default()
        };
        assert_eq!(tick(&mut state, 6_750).len(), 1);
        let events = tick(&mut state, 250);
        assert_eq!(events[1..], [PlayerEvent::Started { uid: 2 }]);
        assert_eq!(state.position_ms, 0);
        let last = tick(&mut state, 10_000);
        assert_eq!(
            last[1..],
            [PlayerEvent::TrackEnded { uid: 2 }, PlayerEvent::Idle]
        );
    }

    #[test]
    fn fake_live_streams_never_end() {
        let mut state = FakeState {
            current: Some(track(1, 0)),
            is_playing: true,
            ..FakeState::default()
        };
        for _ in 0..10 {
            tick(&mut state, 250);
        }
        assert_eq!(
            tick(&mut state, 250),
            [PlayerEvent::Position {
                position_ms: 2750,
                duration_ms: None
            }]
        );
    }
}
