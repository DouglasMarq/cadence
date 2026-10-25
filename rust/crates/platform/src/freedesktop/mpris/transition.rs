use std::time::Instant;

use mpris_server::{Metadata, PlaybackStatus as MprisStatus, Property, Time};

use super::Update;
use super::conversions::{split_status, time_from_ms};
use super::metadata::metadata_for;
use super::snapshot::PlaybackSnapshot;
use crate::{NowPlaying, PlaybackStatus};

const SEEK_DETECT_THRESHOLD_MS: u64 = 1000;

#[derive(Default)]
pub(super) struct UpdateSignals {
    pub(super) properties: Vec<Property>,
    pub(super) seeked_to: Option<Time>,
}

pub(super) enum SnapshotChange {
    Unchanged,
    Metadata(Metadata),
    Status {
        status: MprisStatus,
        position_ms: u64,
        at: Instant,
    },
    Volume(f64),
    LiveStream(bool),
}

pub(super) struct SnapshotTransition {
    pub(super) change: SnapshotChange,
    pub(super) signals: UpdateSignals,
}

impl SnapshotTransition {
    fn unchanged() -> Self {
        Self::emitting(SnapshotChange::Unchanged, Vec::new())
    }

    fn emitting(change: SnapshotChange, properties: Vec<Property>) -> Self {
        Self {
            change,
            signals: UpdateSignals {
                properties,
                seeked_to: None,
            },
        }
    }
}

impl PlaybackSnapshot {
    pub(super) fn transition_for(&self, update: Update, now: Instant) -> SnapshotTransition {
        match update {
            Update::Metadata(now_playing) => metadata_transition(&now_playing),
            Update::Status(status) => self.status_transition(status, now),
            Update::Volume(volume) => self.volume_transition(volume),
            Update::LiveStream(is_live) => self.live_stream_transition(is_live),
            Update::Emit(properties) => {
                SnapshotTransition::emitting(SnapshotChange::Unchanged, properties)
            }
        }
    }

    pub(super) fn apply(&mut self, change: SnapshotChange) {
        match change {
            SnapshotChange::Unchanged => {}
            SnapshotChange::Metadata(metadata) => self.metadata = metadata,
            SnapshotChange::Status {
                status,
                position_ms,
                at,
            } => self.set_status(status, position_ms, at),
            SnapshotChange::Volume(volume) => self.volume = volume,
            SnapshotChange::LiveStream(is_live) => self.is_live_stream = is_live,
        }
    }

    fn status_transition(&self, status: PlaybackStatus, now: Instant) -> SnapshotTransition {
        let (status, position_ms) = split_status(status);
        let expected_position_ms = self.position_ms(now);
        let has_seeked = status != MprisStatus::Stopped
            && expected_position_ms.abs_diff(position_ms) > SEEK_DETECT_THRESHOLD_MS;
        let has_status_changed = self.status != status;
        SnapshotTransition {
            change: SnapshotChange::Status {
                status,
                position_ms,
                at: now,
            },
            signals: UpdateSignals {
                properties: has_status_changed
                    .then_some(Property::PlaybackStatus(status))
                    .into_iter()
                    .collect(),
                seeked_to: has_seeked.then(|| time_from_ms(position_ms)),
            },
        }
    }

    fn live_stream_transition(&self, is_live: bool) -> SnapshotTransition {
        if self.is_live_stream == is_live {
            return SnapshotTransition::unchanged();
        }
        SnapshotTransition::emitting(
            SnapshotChange::LiveStream(is_live),
            vec![
                Property::CanGoNext(!is_live),
                Property::CanGoPrevious(!is_live),
                Property::CanSeek(!is_live),
            ],
        )
    }

    fn volume_transition(&self, volume: f64) -> SnapshotTransition {
        let volume = volume.clamp(0.0, 1.0);
        if (self.volume - volume).abs() < f64::EPSILON {
            return SnapshotTransition::unchanged();
        }
        SnapshotTransition::emitting(
            SnapshotChange::Volume(volume),
            vec![Property::Volume(volume)],
        )
    }
}

fn metadata_transition(now_playing: &NowPlaying) -> SnapshotTransition {
    let metadata = metadata_for(now_playing);
    SnapshotTransition::emitting(
        SnapshotChange::Metadata(metadata.clone()),
        vec![Property::Metadata(metadata)],
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn status(position_ms: u64) -> Update {
        Update::Status(PlaybackStatus::Playing { position_ms })
    }

    #[test]
    fn given_a_playing_snapshot_when_the_position_jumps_then_a_seek_is_signalled() {
        let mut snapshot = PlaybackSnapshot::new();
        let now = Instant::now();

        let started = snapshot.transition_for(status(0), now);
        assert!(matches!(
            started.signals.properties.as_slice(),
            [Property::PlaybackStatus(MprisStatus::Playing)]
        ));
        assert_eq!(started.signals.seeked_to, None);
        snapshot.apply(started.change);

        let advanced = snapshot.transition_for(status(1_200), now + Duration::from_millis(1_000));
        assert!(advanced.signals.properties.is_empty());
        assert_eq!(advanced.signals.seeked_to, None);
        snapshot.apply(advanced.change);

        let jumped = snapshot.transition_for(status(60_000), now + Duration::from_millis(2_000));
        assert_eq!(jumped.signals.seeked_to, Some(Time::from_millis(60_000)));
    }

    #[test]
    fn given_a_volume_when_it_is_reported_again_then_only_a_change_is_signalled() {
        let mut snapshot = PlaybackSnapshot::new();
        let now = Instant::now();
        assert!(
            snapshot
                .transition_for(Update::Volume(1.0), now)
                .signals
                .properties
                .is_empty()
        );
        let lowered = snapshot.transition_for(Update::Volume(0.5), now);
        assert!(
            matches!(lowered.signals.properties.as_slice(), [Property::Volume(volume)] if *volume == 0.5)
        );
        snapshot.apply(lowered.change);
        assert!((snapshot.volume - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn given_a_live_stream_when_it_starts_and_stops_then_skipping_and_seeking_follow_it() {
        let mut snapshot = PlaybackSnapshot::new();
        let now = Instant::now();
        assert!(
            snapshot
                .transition_for(Update::LiveStream(false), now)
                .signals
                .properties
                .is_empty()
        );

        let live = snapshot.transition_for(Update::LiveStream(true), now);
        assert!(matches!(
            live.signals.properties.as_slice(),
            [
                Property::CanGoNext(false),
                Property::CanGoPrevious(false),
                Property::CanSeek(false)
            ]
        ));
        snapshot.apply(live.change);
        assert!(snapshot.is_live_stream);
        assert!(
            snapshot
                .transition_for(Update::LiveStream(true), now)
                .signals
                .properties
                .is_empty()
        );

        let ended = snapshot.transition_for(Update::LiveStream(false), now);
        assert!(matches!(
            ended.signals.properties.as_slice(),
            [Property::CanGoNext(true), ..]
        ));
    }

    #[test]
    fn given_properties_to_emit_when_transitioning_then_the_snapshot_is_unchanged() {
        let snapshot = PlaybackSnapshot::new();
        let transition =
            snapshot.transition_for(Update::Emit(vec![Property::CanSeek(true)]), Instant::now());
        assert!(matches!(transition.change, SnapshotChange::Unchanged));
        assert!(matches!(
            transition.signals.properties.as_slice(),
            [Property::CanSeek(true)]
        ));
    }
}
