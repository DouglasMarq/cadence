use std::time::Duration;

use super::state::MixerState;
use crate::crossfade::{
    FADE_STEP, FadeGains, FadeWindow, FaderPlan, StandbyLoad, Transition, plan_before_fade,
    plan_during_fade,
};
use crate::deck::{Deck, DeckSetting};
use crate::error::log_failure;

pub(super) enum FaderStep {
    Idle,
    Recheck(Duration),
    LoadStandby,
    BeginFade {
        window: FadeWindow,
        transition: Transition,
    },
    Ramp(FadeGains),
    FinishFade,
}

impl FaderStep {
    pub(super) fn wait(&self) -> Option<Duration> {
        match self {
            FaderStep::Recheck(delay) => Some(*delay),
            FaderStep::BeginFade { .. } => Some(Duration::ZERO),
            FaderStep::Ramp(_) => Some(FADE_STEP),
            FaderStep::Idle | FaderStep::LoadStandby | FaderStep::FinishFade => None,
        }
    }
}

impl MixerState {
    pub(super) fn next_fader_step(&self) -> FaderStep {
        match self.fade {
            Some(window) => self.ramp_step(window),
            None if !self.config.is_crossfade_enabled() => FaderStep::Idle,
            None if self.routing.standby_is_ready() => self.fade_start_step(),
            None if self.standby_load_is_waiting() => self.standby_load_step(),
            None => FaderStep::Idle,
        }
    }

    pub(super) fn apply_fader_step(&mut self, step: FaderStep) {
        match step {
            FaderStep::Idle | FaderStep::Recheck(_) => {}
            FaderStep::LoadStandby => self.preload_pending_in_background(),
            FaderStep::BeginFade { window, transition } => self.begin_fade(window, transition),
            FaderStep::Ramp(gains) => self.apply_gains(gains),
            FaderStep::FinishFade => self.finish_fade(),
        }
    }

    pub(crate) fn is_crossfading(&self) -> bool {
        self.routing.is_fading()
    }

    pub(crate) fn holds_next_track(&self) -> bool {
        self.routing.has_standby() && !self.routing.is_fading()
    }

    pub(super) fn finish_fade(&mut self) {
        if !self.routing.is_fading() {
            return;
        }
        self.dismiss_standby();
        self.load_standby_in_background_when_due();
    }

    fn standby_load_step(&self) -> FaderStep {
        match self.standby_load_plan() {
            StandbyLoad::Now => FaderStep::LoadStandby,
            StandbyLoad::Later(wait) => FaderStep::Recheck(wait),
        }
    }

    fn fade_start_step(&self) -> FaderStep {
        let Some(lead) = self.lead_clock() else {
            return FaderStep::Idle;
        };
        match plan_before_fade(
            self.config.crossfade,
            lead,
            self.incoming_duration_seconds(),
        ) {
            FaderPlan::Begin(window) => {
                self.routing
                    .after_fade_start()
                    .map_or(FaderStep::Idle, |transition| FaderStep::BeginFade {
                        window,
                        transition,
                    })
            }
            FaderPlan::Recheck(delay) => FaderStep::Recheck(delay),
            FaderPlan::Ramp(_) | FaderPlan::Finish => FaderStep::Idle,
        }
    }

    fn ramp_step(&self, window: FadeWindow) -> FaderStep {
        let incoming_position_seconds = self.lead_deck().ok().and_then(Deck::position_seconds);
        match plan_during_fade(window, incoming_position_seconds) {
            FaderPlan::Ramp(gains) => FaderStep::Ramp(gains),
            FaderPlan::Begin(_) | FaderPlan::Recheck(_) | FaderPlan::Finish => {
                FaderStep::FinishFade
            }
        }
    }

    fn begin_fade(&mut self, window: FadeWindow, transition: Transition) {
        self.fade = Some(window);
        self.apply_transition(transition);
        log_failure(
            self.lead_deck()
                .and_then(|incoming| incoming.apply(&DeckSetting::Paused(false))),
            "starting the incoming track",
        );
    }

    fn apply_gains(&self, gains: FadeGains) {
        self.apply_to_standby(&DeckSetting::Gain(gains.outgoing));
        log_failure(
            self.lead_deck()
                .and_then(|lead| lead.apply(&DeckSetting::Gain(gains.incoming))),
            "fading in",
        );
    }
}
