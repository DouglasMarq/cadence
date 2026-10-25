use super::routing::{DeckSlot, RouteOutcome, Routing, Standby};
use crate::PlayerEvent;

fn forwarded(events: Vec<PlayerEvent>) -> RouteOutcome {
    RouteOutcome {
        events,
        is_promoted: false,
    }
}

fn ended(uid: u64) -> PlayerEvent {
    PlayerEvent::TrackEnded { uid }
}

fn error(uid: u64) -> PlayerEvent {
    PlayerEvent::Error {
        uid: Some(uid),
        message: "broken".into(),
    }
}

fn outcome_of(routing: &Routing, slot: DeckSlot, event: PlayerEvent) -> RouteOutcome {
    routing.after_event(slot, event).outcome
}

fn routed(routing: &Routing, slot: DeckSlot, event: PlayerEvent) -> Routing {
    routing.after_event(slot, event).routing
}

fn preloading(uid: u64) -> Routing {
    let mut routing = Routing::new();
    routing.preload(uid);
    routing
}

fn routing_with_loaded_preload(uid: u64) -> Routing {
    routed(
        &preloading(uid),
        DeckSlot::Second,
        PlayerEvent::Started { uid },
    )
}

fn fading_into(uid: u64) -> Routing {
    routing_with_loaded_preload(uid)
        .after_fade_start()
        .expect("a loaded preload starts a fade")
        .routing
}

#[test]
fn given_no_standby_when_decks_report_events_then_only_the_lead_events_pass_through() {
    let mut routing = Routing::new();
    for event in [
        PlayerEvent::Started { uid: 1 },
        PlayerEvent::Paused,
        PlayerEvent::Resumed,
        PlayerEvent::Idle,
    ] {
        assert_eq!(
            outcome_of(&routing, DeckSlot::Second, event.clone()),
            RouteOutcome::default()
        );
        let transition = routing.after_event(DeckSlot::First, event.clone());
        assert_eq!(transition.outcome, forwarded(vec![event]));
        routing = transition.routing;
    }
}

#[test]
fn given_a_preload_when_files_start_then_only_its_own_file_on_the_standby_makes_it_ready() {
    let routing = preloading(2);
    let routing = routed(&routing, DeckSlot::Second, PlayerEvent::Started { uid: 9 });
    assert!(!routing.standby_is_ready());
    let routing = routed(&routing, DeckSlot::First, PlayerEvent::Started { uid: 2 });
    assert!(!routing.standby_is_ready());
    let routing = routed(&routing, DeckSlot::Second, PlayerEvent::Started { uid: 2 });
    assert!(routing.standby_is_ready());
}

#[test]
fn given_a_loaded_preload_when_the_fade_starts_then_the_lead_swaps_and_the_track_is_announced() {
    assert_eq!(preloading(2).after_fade_start(), None);
    let transition = routing_with_loaded_preload(2)
        .after_fade_start()
        .expect("a loaded preload starts a fade");
    assert_eq!(
        transition.outcome,
        forwarded(vec![PlayerEvent::Started { uid: 2 }])
    );
    assert_eq!(transition.routing.lead(), DeckSlot::Second);
    assert_eq!(transition.routing.standby_slot(), DeckSlot::First);
    assert!(transition.routing.is_fading());
    assert_eq!(transition.routing.after_fade_start(), None);
}

#[test]
fn given_a_fade_when_the_outgoing_track_ends_then_its_end_is_hidden_but_its_errors_are_reported() {
    let routing = fading_into(2);
    assert_eq!(
        outcome_of(&routing, DeckSlot::First, ended(1)),
        RouteOutcome::default()
    );
    assert_eq!(
        outcome_of(&routing, DeckSlot::First, PlayerEvent::Idle),
        RouteOutcome::default()
    );
    assert_eq!(
        outcome_of(&routing, DeckSlot::First, error(1)),
        forwarded(vec![error(1)])
    );
}

#[test]
fn given_a_loaded_preload_without_fade_when_the_lead_ends_then_it_is_promoted_like_gapless() {
    let transition = routing_with_loaded_preload(2).after_event(DeckSlot::First, ended(1));
    assert_eq!(
        transition.outcome,
        RouteOutcome {
            events: vec![ended(1), PlayerEvent::Started { uid: 2 }],
            is_promoted: true,
        }
    );
    assert_eq!(transition.routing.lead(), DeckSlot::Second);
    assert_eq!(
        outcome_of(&transition.routing, DeckSlot::First, PlayerEvent::Idle),
        RouteOutcome::default()
    );
}

#[test]
fn given_a_preload_still_loading_when_the_lead_ends_then_it_is_promoted() {
    let transition = preloading(2).after_event(DeckSlot::First, error(1));
    assert_eq!(
        transition.outcome,
        RouteOutcome {
            events: vec![error(1)],
            is_promoted: true,
        }
    );
    assert_eq!(
        outcome_of(
            &transition.routing,
            DeckSlot::Second,
            PlayerEvent::Started { uid: 2 }
        ),
        forwarded(vec![PlayerEvent::Started { uid: 2 }])
    );
}

#[test]
fn given_a_failed_preload_when_the_lead_ends_then_the_failure_is_reported() {
    let failed = preloading(2).after_event(DeckSlot::Second, error(2));
    assert_eq!(failed.outcome, RouteOutcome::default());
    assert!(!failed.routing.standby_is_ready());
    let transition = failed.routing.after_event(DeckSlot::First, ended(1));
    assert_eq!(transition.outcome, forwarded(vec![ended(1), error(2)]));
    assert_eq!(transition.routing.lead(), DeckSlot::First);
    assert!(!transition.routing.has_standby());
}

#[test]
fn given_a_fade_when_the_lead_ends_then_the_fade_is_kept() {
    let transition = fading_into(2).after_event(DeckSlot::Second, ended(2));
    assert_eq!(transition.outcome, forwarded(vec![ended(2)]));
    assert!(transition.routing.is_fading());
}

#[test]
fn given_a_cleared_standby_when_the_lead_ends_then_its_events_are_no_longer_routed() {
    let mut routing = routing_with_loaded_preload(2);
    routing.clear_standby();
    assert!(!routing.has_standby());
    let transition = routing.after_event(DeckSlot::First, ended(1));
    assert_eq!(transition.outcome, forwarded(vec![ended(1)]));
    assert_eq!(transition.routing.lead(), DeckSlot::First);
    assert_eq!(DeckSlot::First.other().other(), DeckSlot::First);
    assert_ne!(Standby::FadingOut, Standby::Empty);
}

#[test]
fn given_a_fade_when_the_incoming_deck_unpauses_then_no_resume_is_reported() {
    let routing = fading_into(2);
    assert_eq!(
        outcome_of(&routing, DeckSlot::Second, PlayerEvent::Resumed),
        RouteOutcome::default()
    );
    let paused = routing.after_event(DeckSlot::Second, PlayerEvent::Paused);
    assert_eq!(paused.outcome, forwarded(vec![PlayerEvent::Paused]));
    assert_eq!(
        outcome_of(&paused.routing, DeckSlot::Second, PlayerEvent::Paused),
        RouteOutcome::default()
    );
}

#[test]
fn given_a_pause_state_when_the_same_state_is_reported_again_then_nothing_changes() {
    let routing = Routing::new();
    let unchanged = routing.after_pause_change(false);
    assert_eq!(unchanged.outcome, RouteOutcome::default());
    assert_eq!(unchanged.routing, routing);
    let paused = routing.after_pause_change(true);
    assert_eq!(paused.outcome, forwarded(vec![PlayerEvent::Paused]));
    assert_eq!(
        paused.routing.after_pause_change(false).outcome,
        forwarded(vec![PlayerEvent::Resumed])
    );
}

#[test]
fn given_a_fade_when_decks_go_idle_then_only_the_fading_deck_is_released() {
    let idle = PlayerEvent::Idle;
    assert!(!routing_with_loaded_preload(2).outgoing_deck_went_idle(DeckSlot::Second, &idle));
    let routing = fading_into(2);
    assert!(routing.outgoing_deck_went_idle(DeckSlot::First, &idle));
    assert!(!routing.outgoing_deck_went_idle(DeckSlot::First, &ended(1)));
    assert!(!routing.outgoing_deck_went_idle(DeckSlot::First, &error(1)));
    assert!(!routing.outgoing_deck_went_idle(DeckSlot::Second, &idle));
}
