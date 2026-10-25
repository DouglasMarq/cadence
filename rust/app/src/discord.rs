use std::sync::mpsc::{self, Receiver, RecvError, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use cadence_platform::{DiscordPresence, NowPlaying, PlatformError, PlaybackStatus};

use crate::backoff::exponential_delay;
use crate::media::MediaSession;

const FIRST_RETRY: Duration = Duration::from_secs(5);
const MAX_RETRY: Duration = Duration::from_secs(300);

enum Update {
    Metadata(NowPlaying),
    Status(PlaybackStatus, Instant),
}

pub struct DiscordSession {
    updates: Sender<Update>,
}

impl DiscordSession {
    pub fn new(client_id: String) -> Result<Self> {
        let (updates, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("discord-presence".into())
            .spawn(move || run(&client_id, &receiver))?;
        Ok(Self { updates })
    }

    fn send(&self, update: Update) -> Result<()> {
        self.updates
            .send(update)
            .map_err(|_| anyhow!("Discord presence thread is gone"))
    }
}

impl MediaSession for DiscordSession {
    fn set_metadata(&mut self, now_playing: &NowPlaying) -> Result<()> {
        self.send(Update::Metadata(now_playing.clone()))
    }
    fn set_status(&mut self, status: PlaybackStatus) -> Result<()> {
        self.send(Update::Status(status, Instant::now()))
    }
    fn set_volume(&mut self, _volume: f64) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct Backoff {
    failures: u32,
    next_attempt: Instant,
}

impl Backoff {
    fn new(now: Instant) -> Self {
        Self {
            failures: 0,
            next_attempt: now,
        }
    }

    fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        self.next_attempt = now + exponential_delay(FIRST_RETRY, self.failures, MAX_RETRY);
    }

    fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

#[derive(Debug, PartialEq)]
struct Activity<'desired> {
    now_playing: &'desired NowPlaying,
    position_ms: u64,
    is_paused: bool,
}

#[derive(Default)]
struct Desired {
    now_playing: Option<NowPlaying>,
    status: Option<(PlaybackStatus, Instant)>,
}

impl Desired {
    fn apply(&mut self, update: Update) {
        match update {
            Update::Metadata(now_playing) => self.now_playing = Some(now_playing),
            Update::Status(status, received_at) => self.status = Some((status, received_at)),
        }
    }

    fn activity(&self, now: Instant) -> Option<Activity<'_>> {
        let now_playing = self.now_playing.as_ref()?;
        match self.status? {
            (PlaybackStatus::Playing { position_ms }, received_at) => {
                let elapsed_ms = now.saturating_duration_since(received_at).as_millis() as u64;
                Some(Activity {
                    now_playing,
                    position_ms: position_ms.saturating_add(elapsed_ms),
                    is_paused: false,
                })
            }
            (PlaybackStatus::Paused { position_ms }, _) => Some(Activity {
                now_playing,
                position_ms,
                is_paused: true,
            }),
            (PlaybackStatus::Stopped, _) => None,
        }
    }
}

fn run(client_id: &str, updates: &Receiver<Update>) {
    let mut worker = PresenceWorker::new(client_id);
    while worker.receive(updates).is_ok() {
        worker.sync();
    }
}

struct PresenceWorker<'client> {
    client_id: &'client str,
    desired: Desired,
    presence: Option<DiscordPresence>,
    backoff: Backoff,
    is_dirty: bool,
}

impl<'client> PresenceWorker<'client> {
    fn new(client_id: &'client str) -> Self {
        Self {
            client_id,
            desired: Desired::default(),
            presence: None,
            backoff: Backoff::new(Instant::now()),
            is_dirty: false,
        }
    }

    fn receive(&mut self, updates: &Receiver<Update>) -> Result<(), RecvError> {
        match self.wait_for_update(updates) {
            Ok(update) => {
                self.desired.apply(update);
                updates
                    .try_iter()
                    .for_each(|update| self.desired.apply(update));
                self.is_dirty = true;
                Ok(())
            }
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => Err(RecvError),
        }
    }

    fn wait_for_update(&self, updates: &Receiver<Update>) -> Result<Update, RecvTimeoutError> {
        let wants_connection =
            self.presence.is_none() && self.desired.activity(Instant::now()).is_some();
        if !wants_connection {
            return updates.recv().map_err(|_| RecvTimeoutError::Disconnected);
        }
        let wait = self
            .backoff
            .next_attempt
            .saturating_duration_since(Instant::now());
        updates.recv_timeout(wait)
    }

    fn sync(&mut self) {
        let now = Instant::now();
        let is_disconnected = self.presence.is_none();
        if is_disconnected && (!self.is_connect_due(now) || self.connect(now).is_err()) {
            return;
        }
        self.push_activity();
    }

    fn is_connect_due(&self, now: Instant) -> bool {
        self.desired.activity(now).is_some() && now >= self.backoff.next_attempt
    }

    fn connect(&mut self, now: Instant) -> Result<(), PlatformError> {
        match DiscordPresence::connect(self.client_id) {
            Ok(connected) => {
                self.presence = Some(connected);
                self.backoff.reset(now);
                self.is_dirty = true;
                Ok(())
            }
            Err(error) => {
                tracing::debug!("Discord presence: {error}");
                self.backoff.failed(Instant::now());
                Err(error)
            }
        }
    }

    fn push_activity(&mut self) {
        let Some(client) = self.presence.as_mut().filter(|_| self.is_dirty) else {
            return;
        };
        self.is_dirty = false;
        let result = match self.desired.activity(Instant::now()) {
            Some(activity) => client.set_activity(
                activity.now_playing,
                Some(activity.position_ms),
                activity.is_paused,
            ),
            None => client.clear(),
        };
        if let Err(error) = result {
            tracing::debug!("Discord presence: {error}");
            self.presence = None;
            self.backoff.failed(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let start = Instant::now();
        let mut backoff = Backoff::new(start);
        assert_eq!(backoff.next_attempt, start);
        backoff.failed(start);
        assert_eq!(backoff.next_attempt, start + FIRST_RETRY);
        for _ in 0..20 {
            backoff.failed(start);
        }
        assert_eq!(backoff.next_attempt, start + MAX_RETRY);
        backoff.reset(start);
        backoff.failed(start);
        assert_eq!(backoff.next_attempt, start + FIRST_RETRY);
    }

    #[test]
    fn given_a_closed_channel_when_receiving_then_the_worker_stops() {
        let (sender, receiver) = mpsc::channel();
        let mut worker = PresenceWorker::new("client");
        sender
            .send(Update::Status(PlaybackStatus::Stopped, Instant::now()))
            .unwrap();
        drop(sender);

        assert!(worker.receive(&receiver).is_ok());
        assert!(worker.receive(&receiver).is_err());
    }

    #[test]
    fn given_something_to_show_when_the_backoff_allows_then_connecting_is_due() {
        let mut worker = PresenceWorker::new("client");
        let now = Instant::now();
        assert!(!worker.is_connect_due(now));

        worker
            .desired
            .apply(Update::Metadata(NowPlaying::default()));
        worker.desired.apply(Update::Status(
            PlaybackStatus::Paused { position_ms: 0 },
            now,
        ));
        assert!(worker.is_connect_due(now));

        worker.backoff.failed(now);
        assert!(!worker.is_connect_due(now));
    }

    #[test]
    fn activity_follows_status_and_advances_while_playing() {
        let start = Instant::now();
        let mut desired = Desired::default();
        desired.apply(Update::Status(
            PlaybackStatus::Playing { position_ms: 1_000 },
            start,
        ));
        assert!(desired.activity(start).is_none());

        let song = NowPlaying {
            track_id: "s1".into(),
            title: "Song".into(),
            ..NowPlaying::default()
        };
        desired.apply(Update::Metadata(song.clone()));
        assert_eq!(
            desired.activity(start + Duration::from_secs(2)),
            Some(Activity {
                now_playing: &song,
                position_ms: 3_000,
                is_paused: false,
            })
        );

        desired.apply(Update::Status(
            PlaybackStatus::Paused { position_ms: 4_000 },
            start,
        ));
        assert_eq!(
            desired.activity(start + Duration::from_secs(9)),
            Some(Activity {
                now_playing: &song,
                position_ms: 4_000,
                is_paused: true,
            })
        );

        desired.apply(Update::Status(PlaybackStatus::Stopped, start));
        assert!(desired.activity(start).is_none());
    }
}
