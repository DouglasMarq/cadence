use std::sync::{Arc, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::Mixer;
use super::state::MixerState;
use crate::error::{PlayerError, Result};

const FADER_THREAD_NAME: &str = "crossfade";

pub(crate) fn spawn_fader(mixer: Arc<Mixer>) -> Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(FADER_THREAD_NAME.into())
        .spawn(move || run_fader(&mixer))
        .map_err(|source| PlayerError::Thread {
            name: FADER_THREAD_NAME,
            source,
        })
}

fn run_fader(mixer: &Mixer) {
    let mut state = mixer.lock_state();
    while !state.is_shutting_down {
        let step = state.next_fader_step();
        let wait = step.wait();
        state.apply_fader_step(step);
        let events = std::mem::take(&mut state.unreported_events);
        if !events.is_empty() {
            drop(state);
            mixer.emit(events);
            state = mixer.lock_state();
            continue;
        }
        if std::mem::take(&mut state.is_fader_woken) {
            continue;
        }
        state = wait_for_wake(mixer, state, wait);
    }
}

fn wait_for_wake<'state>(
    mixer: &'state Mixer,
    state: MutexGuard<'state, MixerState>,
    timeout: Option<Duration>,
) -> MutexGuard<'state, MixerState> {
    let mut state = match timeout {
        Some(timeout) => mixer
            .fader_wake
            .wait_timeout(state, timeout)
            .map_or_else(|poisoned| poisoned.into_inner().0, |(guard, _)| guard),
        None => mixer
            .fader_wake
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner),
    };
    state.is_fader_woken = false;
    state
}
