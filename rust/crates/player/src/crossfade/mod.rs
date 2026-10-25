mod gain;
mod routing;
mod schedule;
mod standby_load;

#[cfg(test)]
mod routing_tests;

pub(crate) use gain::{FadeGains, Gain};
pub(crate) use routing::{DeckSlot, Routing, Transition, ends_track};
pub(crate) use schedule::{
    FADE_STEP, FadeWindow, FaderPlan, OUTPUT_BUFFER_SECONDS, plan_before_fade, plan_during_fade,
};
pub(crate) use standby_load::{StandbyLoad, plan_standby_load};
