use cadence_core::queue::Uid;

use crate::PlayerEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeckSlot {
    First,
    Second,
}

impl DeckSlot {
    pub(crate) fn other(self) -> Self {
        match self {
            DeckSlot::First => DeckSlot::Second,
            DeckSlot::Second => DeckSlot::First,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Standby {
    Empty,
    Preloading { uid: Uid, is_loaded: bool },
    Failed { uid: Uid, message: String },
    FadingOut,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RouteOutcome {
    pub(crate) events: Vec<PlayerEvent>,
    pub(crate) is_promoted: bool,
}

impl RouteOutcome {
    fn forward(event: PlayerEvent) -> Self {
        Self {
            events: vec![event],
            is_promoted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Transition {
    pub(crate) routing: Routing,
    pub(crate) outcome: RouteOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Routing {
    lead: DeckSlot,
    standby: Standby,
    is_paused: bool,
}

pub(crate) fn ends_track(event: &PlayerEvent) -> bool {
    matches!(
        event,
        PlayerEvent::TrackEnded { .. } | PlayerEvent::Error { uid: Some(_), .. }
    )
}

impl Routing {
    pub(crate) fn new() -> Self {
        Self {
            lead: DeckSlot::First,
            standby: Standby::Empty,
            is_paused: false,
        }
    }

    pub(crate) fn lead(&self) -> DeckSlot {
        self.lead
    }

    pub(crate) fn standby_slot(&self) -> DeckSlot {
        self.lead.other()
    }

    pub(crate) fn is_fading(&self) -> bool {
        self.standby == Standby::FadingOut
    }

    pub(crate) fn has_standby(&self) -> bool {
        self.standby != Standby::Empty
    }

    pub(crate) fn standby_is_ready(&self) -> bool {
        matches!(
            self.standby,
            Standby::Preloading {
                is_loaded: true,
                ..
            }
        )
    }

    pub(crate) fn preload(&mut self, uid: Uid) {
        self.standby = Standby::Preloading {
            uid,
            is_loaded: false,
        };
    }

    pub(crate) fn preload_failed(&mut self, uid: Uid, message: String) {
        self.standby = Standby::Failed { uid, message };
    }

    pub(crate) fn clear_standby(&mut self) {
        self.standby = Standby::Empty;
    }

    pub(crate) fn after_fade_start(&self) -> Option<Transition> {
        let Standby::Preloading {
            uid,
            is_loaded: true,
        } = self.standby
        else {
            return None;
        };
        Some(Transition {
            routing: Routing {
                lead: self.lead.other(),
                standby: Standby::FadingOut,
                ..self.clone()
            },
            outcome: RouteOutcome::forward(PlayerEvent::Started { uid }),
        })
    }

    pub(crate) fn outgoing_deck_went_idle(&self, slot: DeckSlot, event: &PlayerEvent) -> bool {
        slot != self.lead && self.is_fading() && *event == PlayerEvent::Idle
    }

    pub(crate) fn after_event(&self, slot: DeckSlot, event: PlayerEvent) -> Transition {
        match slot == self.lead {
            true => self.after_lead_event(event),
            false => self.after_standby_event(event),
        }
    }

    fn after_lead_event(&self, event: PlayerEvent) -> Transition {
        match event {
            PlayerEvent::Paused => self.after_pause_change(true),
            PlayerEvent::Resumed => self.after_pause_change(false),
            ended if ends_track(&ended) => self.after_lead_track_ended(ended),
            other => self.unchanged(RouteOutcome::forward(other)),
        }
    }

    pub(crate) fn after_pause_change(&self, is_paused: bool) -> Transition {
        if self.is_paused == is_paused {
            return self.unchanged(RouteOutcome::default());
        }
        Transition {
            routing: Routing {
                is_paused,
                ..self.clone()
            },
            outcome: RouteOutcome::forward(PlayerEvent::pause_change(is_paused)),
        }
    }

    fn after_lead_track_ended(&self, event: PlayerEvent) -> Transition {
        match &self.standby {
            Standby::Preloading { uid, is_loaded } => Transition {
                routing: Routing {
                    lead: self.lead.other(),
                    standby: Standby::Empty,
                    ..self.clone()
                },
                outcome: RouteOutcome {
                    events: [
                        Some(event),
                        is_loaded.then_some(PlayerEvent::Started { uid: *uid }),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                    is_promoted: true,
                },
            },
            Standby::Failed { uid, message } => Transition {
                routing: self.with_standby(Standby::Empty),
                outcome: RouteOutcome {
                    events: vec![
                        event,
                        PlayerEvent::Error {
                            uid: Some(*uid),
                            message: message.clone(),
                        },
                    ],
                    is_promoted: false,
                },
            },
            Standby::Empty | Standby::FadingOut => self.unchanged(RouteOutcome::forward(event)),
        }
    }

    fn after_standby_event(&self, event: PlayerEvent) -> Transition {
        match (&self.standby, event) {
            (Standby::Preloading { uid, .. }, PlayerEvent::Started { uid: started_uid })
                if *uid == started_uid =>
            {
                self.changed_silently(Standby::Preloading {
                    uid: started_uid,
                    is_loaded: true,
                })
            }
            (
                Standby::Preloading { uid, .. },
                PlayerEvent::Error {
                    uid: Some(failed_uid),
                    message,
                },
            ) if *uid == failed_uid => self.changed_silently(Standby::Failed {
                uid: failed_uid,
                message,
            }),
            (Standby::FadingOut, error @ PlayerEvent::Error { .. }) => {
                self.unchanged(RouteOutcome::forward(error))
            }
            _ => self.unchanged(RouteOutcome::default()),
        }
    }

    fn with_standby(&self, standby: Standby) -> Routing {
        Routing {
            standby,
            ..self.clone()
        }
    }

    fn changed_silently(&self, standby: Standby) -> Transition {
        Transition {
            routing: self.with_standby(standby),
            outcome: RouteOutcome::default(),
        }
    }

    fn unchanged(&self, outcome: RouteOutcome) -> Transition {
        Transition {
            routing: self.clone(),
            outcome,
        }
    }
}
