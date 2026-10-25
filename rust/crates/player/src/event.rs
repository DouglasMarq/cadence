use cadence_core::queue::Uid;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Started {
        uid: Uid,
    },
    Position {
        position_ms: u64,
        duration_ms: Option<u64>,
    },
    Paused,
    Resumed,
    TrackEnded {
        uid: Uid,
    },
    Error {
        uid: Option<Uid>,
        message: String,
    },
    Idle,
    StreamTitle(Option<String>),
}

impl PlayerEvent {
    pub(crate) fn pause_change(is_paused: bool) -> Self {
        match is_paused {
            true => PlayerEvent::Paused,
            false => PlayerEvent::Resumed,
        }
    }
}
