use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use mpris_server::{PlaybackStatus as MprisStatus, Property};
use tokio::sync::mpsc;

use super::snapshot::PlaybackSnapshot;
use super::{EventCallback, Update};
use crate::MediaControlEvent;

pub(super) struct MprisPlayerCore {
    pub(super) identity: String,
    pub(super) bus_suffix: String,
    pub(super) desktop_entry: String,
    pub(super) snapshot: Mutex<PlaybackSnapshot>,
    pub(super) on_event: EventCallback,
    pub(super) updates: mpsc::WeakUnboundedSender<Update>,
}

impl MprisPlayerCore {
    pub(super) fn snapshot(&self) -> MutexGuard<'_, PlaybackSnapshot> {
        self.snapshot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn emit(&self, event: MediaControlEvent) {
        (self.on_event)(event);
    }

    pub(super) fn notify(&self, properties: Vec<Property>) {
        let Some(updates) = self.updates.upgrade() else {
            return;
        };
        if updates.send(Update::Emit(properties)).is_err() {
            tracing::debug!("MPRIS thread stopped before a property change could be emitted");
        }
    }

    pub(super) fn update_status(&self, next: impl FnOnce(MprisStatus) -> MprisStatus) {
        let now = Instant::now();
        let mut snapshot = self.snapshot();
        let status = next(snapshot.status);
        if status == snapshot.status {
            return;
        }
        let position_ms = snapshot.position_ms(now);
        snapshot.set_status(status, position_ms, now);
        drop(snapshot);
        self.notify(vec![Property::PlaybackStatus(status)]);
    }

    pub(super) fn transport(
        &self,
        event: MediaControlEvent,
        next: impl FnOnce(MprisStatus) -> MprisStatus,
    ) {
        if !self.has_track() {
            return;
        }
        self.emit(event);
        self.update_status(next);
    }

    fn has_track(&self) -> bool {
        self.snapshot().has_track()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::metadata::metadata_for;
    use super::super::transition::SnapshotChange;
    use super::*;
    use crate::NowPlaying;

    fn song() -> NowPlaying {
        NowPlaying {
            track_id: "a1b2-c3d4.e5".to_owned(),
            title: "Song".to_owned(),
            artists: vec!["Artist A".to_owned(), "Artist B".to_owned()],
            album: Some("Album".to_owned()),
            duration_ms: Some(215_500),
            cover_url: Some("https://example.com/cover.jpg".to_owned()),
            cover_file: None,
        }
    }

    type UpdateChannel = (
        mpsc::UnboundedSender<Update>,
        mpsc::UnboundedReceiver<Update>,
    );

    fn player_core(on_event: EventCallback) -> (MprisPlayerCore, UpdateChannel) {
        let (update_sender, update_receiver) = mpsc::unbounded_channel();
        let core = MprisPlayerCore {
            identity: "Cadence".to_owned(),
            bus_suffix: "cadence".to_owned(),
            desktop_entry: "org.example.cadence".to_owned(),
            snapshot: Mutex::new(PlaybackSnapshot::new()),
            on_event,
            updates: update_sender.downgrade(),
        };
        (core, (update_sender, update_receiver))
    }

    #[test]
    fn transport_requires_a_track() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (core, (_update_sender, mut update_receiver)) = player_core(Box::new(move |event| {
            event_sink.lock().unwrap().push(event);
        }));

        core.transport(MediaControlEvent::Toggle, |_| MprisStatus::Playing);
        assert!(events.lock().unwrap().is_empty());
        assert_eq!(core.snapshot().status, MprisStatus::Stopped);

        core.snapshot()
            .apply(SnapshotChange::Metadata(metadata_for(&song())));
        core.transport(MediaControlEvent::Toggle, |_| MprisStatus::Playing);
        assert_eq!(*events.lock().unwrap(), vec![MediaControlEvent::Toggle]);
        assert_eq!(core.snapshot().status, MprisStatus::Playing);
        assert!(matches!(
            update_receiver.try_recv(),
            Ok(Update::Emit(properties)) if matches!(properties.as_slice(), [Property::PlaybackStatus(MprisStatus::Playing)])
        ));
    }
}
