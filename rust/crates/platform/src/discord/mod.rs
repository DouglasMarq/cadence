mod activity;
mod connection;
mod options;
mod text_field;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use cadence_core::unix_time::unix_now_ms;
use discord_rich_presence::DiscordIpcClient;

use crate::{NowPlaying, PlatformError, Result};
use activity::{PlaybackMoment, activity_for};
use connection::{Command, spawn_connection_thread};
pub use options::{DiscordDisplayType, DiscordOptions};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct DiscordPresence {
    commands: mpsc::Sender<Command>,
    options: DiscordOptions,
}

impl DiscordPresence {
    pub fn connect(client_id: &str) -> Result<Self> {
        let client = DiscordIpcClient::new(client_id);
        let (commands, command_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        spawn_connection_thread(client, command_receiver, ready_sender)?;
        match ready_receiver.recv_timeout(CONNECT_TIMEOUT) {
            Ok(connected) => connected.map(|()| Self {
                commands,
                options: DiscordOptions::default(),
            }),
            Err(RecvTimeoutError::Timeout) => Err(PlatformError::Discord(
                "timed out connecting to Discord".into(),
            )),
            Err(RecvTimeoutError::Disconnected) => Err(PlatformError::Discord(
                "the Discord thread stopped before connecting".into(),
            )),
        }
    }

    pub fn options(&self) -> &DiscordOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: DiscordOptions) {
        self.options = options;
    }

    pub fn set_activity(
        &mut self,
        now_playing: &NowPlaying,
        position_ms: Option<u64>,
        is_paused: bool,
    ) -> Result<()> {
        let playback = PlaybackMoment {
            position_ms,
            is_paused,
            now_ms: unix_now_ms(),
        };
        let activity = activity_for(now_playing, playback, &self.options);
        let command = match activity {
            Some(activity) => Command::Set(Box::new(activity)),
            None => Command::Clear,
        };
        self.send(command)
    }

    pub fn clear(&mut self) -> Result<()> {
        self.send(Command::Clear)
    }

    fn send(&self, command: Command) -> Result<()> {
        self.commands
            .send(command)
            .map_err(|error| PlatformError::Discord(Box::new(error)))
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        if self.commands.send(Command::Close).is_err() {
            tracing::debug!("Discord thread already stopped before close");
        }
    }
}
