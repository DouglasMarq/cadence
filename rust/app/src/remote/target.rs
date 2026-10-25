use std::sync::{Arc, PoisonError, RwLock};

use cadence_core::queue::QueueTrack;
use cadence_remote::{ClientMessage, RemoteControl, ServerKey, TrackPlacement};
use tokio::sync::mpsc;

use super::lists::{remote_list_of, remote_placement_of};
use crate::controller::{Command, TrackCommand};

#[derive(Debug, Clone)]
pub struct RemoteTarget {
    pub messages: mpsc::UnboundedSender<ClientMessage>,
}

impl RemoteTarget {
    pub fn send(&self, control: RemoteControl) {
        if self
            .messages
            .send(ClientMessage::Control { control })
            .is_err()
        {
            tracing::debug!("the remote player link is closed, dropping a command");
        }
    }
}

pub enum Routed {
    Remote {
        target: RemoteTarget,
        control: RemoteControl,
    },
    Local(Command),
}

#[derive(Clone, Default)]
pub struct PlayerTarget(Arc<RwLock<Option<RemoteTarget>>>);

impl PlayerTarget {
    pub fn remote(&self) -> Option<RemoteTarget> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn is_remote(&self) -> bool {
        self.remote().is_some()
    }

    pub fn set(&self, target: Option<RemoteTarget>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = target;
    }

    pub fn route(&self, command: Command, local_server: Option<&ServerKey>) -> Routed {
        let Some(target) = self.remote() else {
            return Routed::Local(command);
        };
        match remote_control_of(command, local_server) {
            Ok(control) => Routed::Remote { target, control },
            Err(local) => Routed::Local(*local),
        }
    }
}

pub fn remote_control_of(
    command: Command,
    local_server: Option<&ServerKey>,
) -> Result<RemoteControl, Box<Command>> {
    let server = local_server.cloned();
    match command {
        Command::TogglePause => Ok(RemoteControl::TogglePause),
        Command::Next => Ok(RemoteControl::Next),
        Command::Previous => Ok(RemoteControl::Previous),
        Command::Seek(position_ms) => Ok(RemoteControl::Seek { position_ms }),
        Command::SetVolume(volume) => Ok(RemoteControl::SetVolume { volume }),
        Command::ToggleMute => Ok(RemoteControl::ToggleMute),
        Command::ToggleShuffle => Ok(RemoteControl::ToggleShuffle),
        Command::CycleRepeat => Ok(RemoteControl::CycleRepeat),
        Command::PlayUid(uid) => Ok(RemoteControl::PlayUid { uid }),
        Command::Remove(uids) => Ok(RemoteControl::Remove { uids }),
        Command::Move { uids, before } => Ok(RemoteControl::Move { uids, before }),
        Command::PlaySongs { songs, song_id } => Ok(RemoteControl::PlayTracks {
            start_index: song_id.and_then(|wanted| songs.iter().position(|song| song.id == wanted)),
            tracks: songs.iter().map(QueueTrack::from).collect(),
            server,
        }),
        Command::AddNext(songs) => Ok(add_tracks(
            songs.iter().map(QueueTrack::from).collect(),
            TrackPlacement::Next,
            server,
        )),
        Command::AddLast(songs) => Ok(add_tracks(
            songs.iter().map(QueueTrack::from).collect(),
            TrackPlacement::Last,
            server,
        )),
        Command::Tracks(tracks) => Ok(track_control(tracks, server)),
        command @ (Command::PlayFromList { .. } | Command::EnqueueFromList { .. }) => {
            list_control(command, server)
        }
        other => Err(Box::new(other)),
    }
}

fn add_tracks(
    tracks: Vec<QueueTrack>,
    placement: TrackPlacement,
    server: Option<ServerKey>,
) -> RemoteControl {
    RemoteControl::AddTracks {
        tracks,
        placement,
        server,
    }
}

fn track_control(command: TrackCommand, server: Option<ServerKey>) -> RemoteControl {
    match command {
        TrackCommand::PlayNow {
            tracks,
            start_index,
        } => RemoteControl::PlayTracks {
            tracks,
            start_index,
            server,
        },
        TrackCommand::AddNext(tracks) => add_tracks(tracks, TrackPlacement::Next, server),
        TrackCommand::AddLast(tracks) => add_tracks(tracks, TrackPlacement::Last, server),
    }
}

fn list_control(
    command: Command,
    server: Option<ServerKey>,
) -> Result<RemoteControl, Box<Command>> {
    let Some(server) = server else {
        return Err(Box::new(command));
    };
    match command {
        Command::PlayFromList {
            source,
            song_id,
            row_index,
        } => Ok(RemoteControl::PlayList {
            list: remote_list_of(source),
            song_id,
            row: row_index.0,
            server,
        }),
        Command::EnqueueFromList {
            source,
            ranges,
            placement,
        } => Ok(RemoteControl::EnqueueList {
            list: remote_list_of(source),
            ranges,
            placement: remote_placement_of(placement),
            server,
        }),
        other => Err(Box::new(other)),
    }
}

#[cfg(test)]
mod tests {
    use cadence_core::model::Song;

    use super::*;
    use crate::demo::test_song;
    use crate::list_stream::{ListRow, Placement, PlaylistId, SongSource};

    fn server() -> ServerKey {
        ServerKey {
            url: "http://navidrome.example".into(),
            username: "me".into(),
        }
    }

    fn songs() -> Vec<Song> {
        vec![test_song("a", 1_000), test_song("b", 1_000)]
    }

    #[test]
    fn playback_and_queue_commands_go_to_the_remote_player() {
        assert_eq!(
            remote_control_of(Command::Seek(5_000), None).ok(),
            Some(RemoteControl::Seek { position_ms: 5_000 })
        );
        let Ok(RemoteControl::PlayTracks {
            tracks,
            start_index,
            server: sent_server,
        }) = remote_control_of(
            Command::PlaySongs {
                songs: songs(),
                song_id: Some("b".into()),
            },
            Some(&server()),
        )
        else {
            panic!("expected PlayTracks");
        };
        assert_eq!(
            (tracks.len(), start_index, sent_server),
            (2, Some(1), Some(server()))
        );
    }

    #[test]
    fn lists_need_a_server_and_everything_else_stays_local() {
        let play_list = || Command::PlayFromList {
            source: SongSource::Playlist(PlaylistId("p".into())),
            song_id: None,
            row_index: ListRow(3),
        };
        assert!(matches!(
            remote_control_of(play_list(), Some(&server())),
            Ok(RemoteControl::PlayList { row: 3, .. })
        ));
        assert!(remote_control_of(play_list(), None).is_err());
        assert!(remote_control_of(Command::SaveLanguage(None), Some(&server())).is_err());
    }

    #[test]
    fn given_a_playback_command_when_converted_then_it_becomes_the_matching_control() {
        let cases = [
            (Command::TogglePause, RemoteControl::TogglePause),
            (Command::Next, RemoteControl::Next),
            (Command::Previous, RemoteControl::Previous),
            (
                Command::SetVolume(40),
                RemoteControl::SetVolume { volume: 40 },
            ),
            (Command::ToggleMute, RemoteControl::ToggleMute),
            (Command::ToggleShuffle, RemoteControl::ToggleShuffle),
            (Command::CycleRepeat, RemoteControl::CycleRepeat),
            (Command::PlayUid(4), RemoteControl::PlayUid { uid: 4 }),
            (
                Command::Remove(vec![1, 2]),
                RemoteControl::Remove { uids: vec![1, 2] },
            ),
            (
                Command::Move {
                    uids: vec![3],
                    before: Some(1),
                },
                RemoteControl::Move {
                    uids: vec![3],
                    before: Some(1),
                },
            ),
        ];

        for (command, control) in cases {
            assert_eq!(remote_control_of(command, None).ok(), Some(control));
        }
    }

    fn placement_and_count(control: RemoteControl) -> Option<(TrackPlacement, usize)> {
        match control {
            RemoteControl::AddTracks {
                tracks, placement, ..
            } => Some((placement, tracks.len())),
            _ => None,
        }
    }

    fn tracks() -> Vec<QueueTrack> {
        songs().iter().map(QueueTrack::from).collect()
    }

    #[test]
    fn given_songs_to_add_when_converted_then_they_are_added_at_the_same_place() {
        let commands = [
            Command::AddNext(songs()),
            Command::AddLast(songs()),
            Command::Tracks(TrackCommand::AddNext(tracks())),
            Command::Tracks(TrackCommand::AddLast(tracks())),
        ];

        let placed: Vec<Option<(TrackPlacement, usize)>> = commands
            .into_iter()
            .map(|command| {
                remote_control_of(command, None)
                    .ok()
                    .and_then(placement_and_count)
            })
            .collect();

        assert_eq!(
            placed,
            [
                Some((TrackPlacement::Next, 2)),
                Some((TrackPlacement::Last, 2)),
                Some((TrackPlacement::Next, 2)),
                Some((TrackPlacement::Last, 2)),
            ]
        );
    }

    #[test]
    fn given_tracks_to_play_when_converted_then_they_play_from_the_same_track() {
        let converted = remote_control_of(
            Command::Tracks(TrackCommand::PlayNow {
                tracks: tracks(),
                start_index: Some(1),
            }),
            Some(&server()),
        );

        assert!(matches!(
            converted,
            Ok(RemoteControl::PlayTracks {
                start_index: Some(1),
                server: Some(_),
                ..
            })
        ));
    }

    #[test]
    fn given_a_list_to_enqueue_when_converted_then_it_needs_a_server() {
        let enqueue = || Command::EnqueueFromList {
            source: SongSource::Playlist(PlaylistId("p".into())),
            ranges: vec![0..4, 6..8],
            placement: Placement::Last,
        };

        assert!(matches!(
            remote_control_of(enqueue(), Some(&server())),
            Ok(RemoteControl::EnqueueList {
                placement: cadence_remote::ListPlacement::Last,
                ..
            })
        ));
        assert!(matches!(
            remote_control_of(enqueue(), None).map_err(|command| *command),
            Err(Command::EnqueueFromList { .. })
        ));
    }

    #[test]
    fn given_a_closed_link_when_sending_then_the_command_is_dropped_quietly() {
        let (messages, sent) = mpsc::unbounded_channel();
        drop(sent);
        let target = RemoteTarget { messages };

        target.send(RemoteControl::Next);

        assert!(target.messages.is_closed());
    }

    #[test]
    fn given_a_local_target_when_routing_then_everything_runs_locally() {
        let target = PlayerTarget::default();

        let routed = target.route(Command::Next, None);

        assert!(matches!(routed, Routed::Local(Command::Next)));
    }

    #[test]
    fn given_a_remote_target_when_routing_then_playback_goes_to_it_and_settings_stay_local() {
        let target = PlayerTarget::default();
        let (messages, mut sent) = mpsc::unbounded_channel();
        target.set(Some(RemoteTarget { messages }));

        let Routed::Remote {
            target: remote,
            control,
        } = target.route(Command::Next, None)
        else {
            panic!("expected the command to go to the remote player");
        };
        remote.send(control);

        assert_eq!(
            sent.try_recv().ok(),
            Some(ClientMessage::Control {
                control: RemoteControl::Next
            })
        );
        assert!(matches!(
            target.route(Command::SaveLanguage(None), None),
            Routed::Local(Command::SaveLanguage(None))
        ));
    }
}
