use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use cadence_remote::{
    Controller, DeviceInfo, HostDelegate, RemoteControl, RemoteFailure, ServerKey, TrackPlacement,
    TransferPayload,
};
use tokio::sync::{mpsc, oneshot};

use super::approval::Approvals;
use super::lists::{placement_of, song_source_of};
use super::transfer::{LocalServer, TrackAdapter, adapt_payload};
use crate::controller::{Command, RemoteCommand, TrackCommand, forward_command};
use crate::list_stream::ListRow;
use crate::session::Session;
use crate::ui::{AppWindow, devices, on_ui};

const HAND_OVER_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AppHostDelegate {
    pub commands: mpsc::UnboundedSender<Command>,
    pub session: Session,
    pub approvals: Approvals,
    pub window: slint::Weak<AppWindow>,
}

impl AppHostDelegate {
    fn local_server(&self) -> Option<LocalServer> {
        self.session
            .get()
            .map(|server| LocalServer::of(server.as_ref()))
    }
}

#[async_trait]
impl HostDelegate for AppHostDelegate {
    async fn approve(&self, device: DeviceInfo, peer: IpAddr) -> bool {
        self.approvals.ask(&self.window, &device, peer).await
    }

    fn control(&self, device: &DeviceInfo, control: RemoteControl) -> Result<(), RemoteFailure> {
        let command = command_of(control, self.local_server().as_ref())?;
        tracing::debug!("{} sent a playback command", device.name);
        forward_command(&self.commands, command);
        Ok(())
    }

    async fn adopt(&self, payload: TransferPayload) -> Result<(), RemoteFailure> {
        let adopted = adapt_payload(payload, self.local_server().as_ref())?;
        let command = Command::Remote(RemoteCommand::AdoptTransfer(Box::new(adopted)));
        forward_command(&self.commands, command);
        Ok(())
    }

    async fn hand_over(&self) -> Result<TransferPayload, RemoteFailure> {
        let (reply, replied) = oneshot::channel();
        forward_command(
            &self.commands,
            Command::Remote(RemoteCommand::TakeTransfer(reply)),
        );
        tokio::time::timeout(HAND_OVER_TIMEOUT, replied)
            .await
            .map_err(|_| RemoteFailure::Busy)?
            .map_err(|_| RemoteFailure::Stopped)?
    }

    fn server(&self) -> Option<ServerKey> {
        self.local_server().map(|local| local.key)
    }

    fn controllers_changed(&self, controllers: Vec<Controller>) {
        on_ui(&self.window, move |window| {
            devices::show_controllers(window, &controllers)
        });
    }
}

pub fn command_of(
    control: RemoteControl,
    local: Option<&LocalServer>,
) -> Result<Command, RemoteFailure> {
    match playback_command(&control) {
        Some(command) => Ok(command),
        None => queue_command(control, local),
    }
}

fn playback_command(control: &RemoteControl) -> Option<Command> {
    Some(match control {
        RemoteControl::TogglePause => Command::TogglePause,
        RemoteControl::Next => Command::Next,
        RemoteControl::Previous => Command::Previous,
        RemoteControl::Seek { position_ms } => Command::Seek(*position_ms),
        RemoteControl::SetVolume { volume } => Command::SetVolume(*volume),
        RemoteControl::ToggleMute => Command::ToggleMute,
        RemoteControl::ToggleShuffle => Command::ToggleShuffle,
        RemoteControl::CycleRepeat => Command::CycleRepeat,
        RemoteControl::PlayUid { uid } => Command::PlayUid(*uid),
        RemoteControl::Remove { uids } => Command::Remove(uids.clone()),
        RemoteControl::Move { uids, before } => Command::Move {
            uids: uids.clone(),
            before: *before,
        },
        _ => return None,
    })
}

fn queue_command(
    control: RemoteControl,
    local: Option<&LocalServer>,
) -> Result<Command, RemoteFailure> {
    match control {
        RemoteControl::PlayTracks {
            tracks,
            start_index,
            server,
        } => {
            let adapted =
                TrackAdapter::new(server.as_ref(), local).adapt_tracks(tracks, start_index)?;
            Ok(Command::Tracks(TrackCommand::PlayNow {
                tracks: adapted.tracks,
                start_index: adapted.start_index,
            }))
        }
        RemoteControl::AddTracks {
            tracks,
            placement,
            server,
        } => {
            let tracks = TrackAdapter::new(server.as_ref(), local)
                .adapt_tracks(tracks, None)?
                .tracks;
            Ok(Command::Tracks(match placement {
                TrackPlacement::Next => TrackCommand::AddNext(tracks),
                TrackPlacement::Last => TrackCommand::AddLast(tracks),
            }))
        }
        other => list_command(other, local),
    }
}

fn list_command(
    control: RemoteControl,
    local: Option<&LocalServer>,
) -> Result<Command, RemoteFailure> {
    match control {
        RemoteControl::PlayList {
            list,
            song_id,
            row,
            server,
        } => {
            same_server(&server, local)?;
            Ok(Command::PlayFromList {
                source: song_source_of(list),
                song_id,
                row_index: ListRow(row),
            })
        }
        RemoteControl::EnqueueList {
            list,
            ranges,
            placement,
            server,
        } => {
            same_server(&server, local)?;
            Ok(Command::EnqueueFromList {
                source: song_source_of(list),
                ranges,
                placement: placement_of(placement),
            })
        }
        other => playback_command(&other).ok_or(RemoteFailure::Stopped),
    }
}

fn same_server(server: &ServerKey, local: Option<&LocalServer>) -> Result<(), RemoteFailure> {
    match local {
        None => Err(RemoteFailure::NotConnected),
        Some(local) if local.key == *server => Ok(()),
        Some(_) => Err(RemoteFailure::DifferentServer),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cadence_api::MusicServer;
    use cadence_core::queue::{QueueTrack, YouTubeTrack};
    use cadence_remote::{DeviceId, RemoteList};

    use super::*;
    use crate::demo::FakeServer;

    fn local() -> LocalServer {
        LocalServer {
            key: ServerKey {
                url: "http://navidrome.example".into(),
                username: "me".into(),
            },
            id: Arc::from("local"),
        }
    }

    fn play_list(url: &str) -> RemoteControl {
        RemoteControl::PlayList {
            list: RemoteList::Playlist { id: "p".into() },
            song_id: None,
            row: 2,
            server: ServerKey {
                url: url.into(),
                username: "me".into(),
            },
        }
    }

    #[test]
    fn lists_play_only_from_the_same_server() {
        assert!(matches!(
            command_of(play_list("http://navidrome.example"), Some(&local())),
            Ok(Command::PlayFromList {
                row_index: ListRow(2),
                ..
            })
        ));
        assert_eq!(
            command_of(play_list("http://other"), Some(&local())).err(),
            Some(RemoteFailure::DifferentServer)
        );
        assert_eq!(
            command_of(play_list("http://navidrome.example"), None).err(),
            Some(RemoteFailure::NotConnected)
        );
    }

    #[test]
    fn simple_controls_map_to_their_commands() {
        assert!(matches!(
            command_of(RemoteControl::Seek { position_ms: 9 }, None),
            Ok(Command::Seek(9))
        ));
        assert!(matches!(
            command_of(
                RemoteControl::Move {
                    uids: vec![1],
                    before: Some(3)
                },
                None
            ),
            Ok(Command::Move {
                before: Some(3),
                ..
            })
        ));
    }

    fn youtube_tracks(count: usize) -> Vec<QueueTrack> {
        (0..count)
            .map(|index| {
                QueueTrack::from(&YouTubeTrack {
                    video_id: format!("video{index}"),
                    title: format!("Video {index}"),
                    artists: Vec::new(),
                    album: None,
                    thumbnail_url: None,
                    duration_ms: 60_000,
                })
            })
            .collect()
    }

    fn add_tracks(placement: TrackPlacement) -> RemoteControl {
        RemoteControl::AddTracks {
            tracks: youtube_tracks(2),
            placement,
            server: None,
        }
    }

    #[test]
    fn given_tracks_to_add_when_received_then_they_are_added_at_the_same_place() {
        assert!(matches!(
            command_of(add_tracks(TrackPlacement::Next), None),
            Ok(Command::Tracks(TrackCommand::AddNext(tracks))) if tracks.len() == 2
        ));
        assert!(matches!(
            command_of(add_tracks(TrackPlacement::Last), None),
            Ok(Command::Tracks(TrackCommand::AddLast(tracks))) if tracks.len() == 2
        ));
    }

    #[test]
    fn given_a_list_to_enqueue_when_received_then_it_is_enqueued_only_from_the_same_server() {
        let enqueue = |url: &str| RemoteControl::EnqueueList {
            list: RemoteList::Playlist { id: "p".into() },
            ranges: vec![0..3, 5..6],
            placement: cadence_remote::ListPlacement::Next,
            server: ServerKey {
                url: url.into(),
                username: "me".into(),
            },
        };

        assert!(matches!(
            command_of(enqueue("http://navidrome.example"), Some(&local())),
            Ok(Command::EnqueueFromList {
                placement: crate::list_stream::Placement::Next,
                ..
            })
        ));
        assert_eq!(
            command_of(enqueue("http://other"), Some(&local())).err(),
            Some(RemoteFailure::DifferentServer)
        );
    }

    fn delegate_of(session: Session) -> (AppHostDelegate, mpsc::UnboundedReceiver<Command>) {
        let (commands, forwarded) = mpsc::unbounded_channel();
        let delegate = AppHostDelegate {
            commands,
            session,
            approvals: Approvals::default(),
            window: slint::Weak::default(),
        };
        (delegate, forwarded)
    }

    fn laptop() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId("laptop".into()),
            name: "Laptop".into(),
        }
    }

    #[tokio::test]
    async fn given_approval_is_not_asked_when_a_device_pairs_then_it_is_let_in() {
        let (delegate, _forwarded) = delegate_of(Session::default());
        delegate.approvals.skip_asking(true);

        let is_approved = delegate
            .approve(laptop(), IpAddr::from([192, 168, 0, 9]))
            .await;

        assert!(is_approved);
    }

    #[test]
    fn given_a_connected_server_when_asked_then_the_delegate_names_it() {
        let session = Session::default();
        let server = Arc::new(FakeServer::new(1));
        let expected = ServerKey::of(server.config());
        session.set(Some(server));
        let (delegate, _forwarded) = delegate_of(session);

        assert_eq!(delegate.server(), Some(expected));
    }

    #[test]
    fn given_a_control_when_received_then_its_command_reaches_the_player() {
        let (delegate, mut forwarded) = delegate_of(Session::default());

        let controlled = delegate.control(&laptop(), RemoteControl::Next);

        assert_eq!(controlled, Ok(()));
        assert!(matches!(forwarded.try_recv(), Ok(Command::Next)));
    }

    #[test]
    fn given_a_list_without_a_connected_server_when_received_then_it_is_refused() {
        let (delegate, mut forwarded) = delegate_of(Session::default());

        let controlled = delegate.control(&laptop(), play_list("http://navidrome.example"));

        assert_eq!(controlled, Err(RemoteFailure::NotConnected));
        assert!(forwarded.try_recv().is_err());
    }

    #[tokio::test]
    async fn given_an_empty_queue_when_adopted_then_it_is_refused() {
        let (delegate, mut forwarded) = delegate_of(Session::default());
        let payload = TransferPayload {
            snapshot: cadence_core::queue::Queue::with_seed(1).snapshot(),
            position_ms: 0,
            is_playing: false,
            server: None,
        };

        let adopted = delegate.adopt(payload).await;

        assert_eq!(adopted, Err(RemoteFailure::NothingToTransfer));
        assert!(forwarded.try_recv().is_err());
    }

    #[tokio::test]
    async fn given_the_player_stopped_when_handing_over_then_it_fails_as_stopped() {
        let (delegate, mut forwarded) = delegate_of(Session::default());
        let answering = tokio::spawn(async move {
            let Some(Command::Remote(RemoteCommand::TakeTransfer(reply))) = forwarded.recv().await
            else {
                return;
            };
            drop(reply);
        });

        let handed = delegate.hand_over().await;

        assert_eq!(handed, Err(RemoteFailure::Stopped));
        answering.await.unwrap();
    }
}
