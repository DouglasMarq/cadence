use std::net::IpAddr;
use std::sync::Mutex;

use async_trait::async_trait;
use cadence_core::queue::{Queue, QueueTrack, YouTubeTrack};
use cadence_platform::MediaControlEvent;
use cadence_remote::{
    Controller, DeviceId, Host, HostDelegate, PairingToken, RemoteControl, RemoteFailure,
    ServerKey, TransferPayload, TrustStore,
};
use tokio::net::TcpListener;

use super::*;
use crate::controller::RemoteCommand;
use crate::remote::approval::Approvals;
use crate::remote::link::LinkEvent;
use crate::store::JsonStore;

const KITCHEN: [u8; 4] = [192, 168, 0, 7];
const WAIT: Duration = Duration::from_secs(5);

struct SpeakerDelegate {
    adopted: Mutex<Vec<TransferPayload>>,
    adopting: Result<(), RemoteFailure>,
    handing_over: Result<TransferPayload, RemoteFailure>,
    controlling: Result<(), RemoteFailure>,
}

impl Default for SpeakerDelegate {
    fn default() -> Self {
        Self {
            adopted: Mutex::new(Vec::new()),
            adopting: Ok(()),
            handing_over: Ok(payload_of(2, 7_000)),
            controlling: Ok(()),
        }
    }
}

#[async_trait]
impl HostDelegate for SpeakerDelegate {
    async fn approve(&self, _device: DeviceInfo, _peer: IpAddr) -> bool {
        true
    }
    fn control(&self, _device: &DeviceInfo, _control: RemoteControl) -> Result<(), RemoteFailure> {
        self.controlling
    }
    async fn adopt(&self, payload: TransferPayload) -> Result<(), RemoteFailure> {
        self.adopted.lock().unwrap().push(payload);
        self.adopting
    }
    async fn hand_over(&self) -> Result<TransferPayload, RemoteFailure> {
        self.handing_over.clone()
    }
    fn server(&self) -> Option<ServerKey> {
        None
    }
    fn controllers_changed(&self, _controllers: Vec<Controller>) {}
}

struct Shown(mpsc::UnboundedSender<crate::controller::UiUpdate>);

impl UiSink for Shown {
    fn send(&self, update: crate::controller::UiUpdate) {
        self.0.send(update).ok();
    }
}

fn payload_of(count: usize, position_ms: u64) -> TransferPayload {
    let mut queue = Queue::with_seed(5);
    let tracks: Vec<QueueTrack> = (0..count)
        .map(|index| {
            QueueTrack::from(&YouTubeTrack {
                video_id: format!("video{index}"),
                title: format!("Video {index}"),
                artists: Vec::new(),
                album: None,
                thumbnail_url: None,
                duration_ms: 100_000,
            })
        })
        .collect();
    queue.play_tracks_now(tracks, Some(0));
    TransferPayload {
        snapshot: queue.snapshot(),
        position_ms,
        is_playing: true,
        server: None,
    }
}

fn device(id: &str) -> DeviceInfo {
    DeviceInfo {
        id: DeviceId(id.into()),
        name: id.into(),
    }
}

fn speaker_id() -> DeviceId {
    DeviceId("speaker".into())
}

struct Setup {
    service: RemoteService,
    commands: mpsc::UnboundedReceiver<Command>,
    speaker: Arc<SpeakerDelegate>,
    speaker_host: Arc<Host>,
    shown: mpsc::UnboundedReceiver<crate::controller::UiUpdate>,
    target: PlayerTarget,
    trust: Arc<FileTrust>,
    _store: tempfile::TempDir,
}

struct Speaker {
    delegate: Arc<SpeakerDelegate>,
    host: Arc<Host>,
    address: std::net::SocketAddr,
    state: watch::Receiver<HostState>,
    store: Arc<dyn crate::store::Store>,
    store_dir: tempfile::TempDir,
}

async fn start_speaker(delegate: SpeakerDelegate) -> Speaker {
    let delegate = Arc::new(delegate);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn crate::store::Store> = Arc::new(JsonStore::new(store_dir.path()));
    let (_state, state) = watch::channel(HostState::default());
    let trust = Arc::new(FileTrust::ephemeral(Arc::clone(&store)));
    let host = Host::new(device("speaker"), delegate.clone(), trust, state.clone());
    tokio::spawn(Arc::clone(&host).serve(listener));
    Speaker {
        delegate,
        host,
        address,
        state,
        store,
        store_dir,
    }
}

fn client_deps(
    speaker: &Speaker,
    commands: mpsc::UnboundedSender<Command>,
    mirror: Arc<Shown>,
) -> ServiceDeps {
    ServiceDeps {
        commands: commands.clone(),
        session: Session::default(),
        window: slint::Weak::default(),
        trust: Arc::new(FileTrust::ephemeral(Arc::clone(&speaker.store))),
        me: device("laptop"),
        target: PlayerTarget::default(),
        mirror,
        host_state: speaker.state.clone(),
        delegate: Arc::new(AppHostDelegate {
            commands,
            session: Session::default(),
            approvals: Approvals::default(),
            window: slint::Weak::default(),
        }),
    }
}

async fn setup() -> Setup {
    setup_with(SpeakerDelegate::default()).await
}

async fn setup_with(delegate: SpeakerDelegate) -> Setup {
    let speaker = start_speaker(delegate).await;
    let (commands_sender, commands) = mpsc::unbounded_channel();
    let (shown_sender, shown) = mpsc::unbounded_channel();
    let mirror = Arc::new(Shown(shown_sender));
    let deps = client_deps(&speaker, commands_sender, mirror);
    let target = deps.target.clone();
    let trust = Arc::clone(&deps.trust);
    let service = RemoteService {
        deps,
        discovery: None,
        found: vec![Discovered {
            device: device("speaker"),
            address: speaker.address,
        }],
        hosting: None,
        link: None,
    };
    Setup {
        service,
        commands,
        speaker: speaker.delegate,
        speaker_host: speaker.host,
        shown,
        target,
        trust,
        _store: speaker.store_dir,
    }
}

async fn next_command(commands: &mut mpsc::UnboundedReceiver<Command>) -> Command {
    tokio::time::timeout(WAIT, commands.recv())
        .await
        .unwrap()
        .unwrap()
}

fn pending_commands(commands: &mut mpsc::UnboundedReceiver<Command>) -> Vec<Command> {
    std::iter::from_fn(|| commands.try_recv().ok()).collect()
}

fn answer_transfers(
    mut commands: mpsc::UnboundedReceiver<Command>,
    answer: Option<Result<TransferPayload, RemoteFailure>>,
) -> mpsc::UnboundedReceiver<Command> {
    let (others_sender, others) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            match (command, &answer) {
                (Command::Remote(RemoteCommand::TakeTransfer(reply)), Some(answer)) => {
                    reply.send(answer.clone()).ok();
                }
                (Command::Remote(RemoteCommand::TakeTransfer(_dropped)), None) => {}
                (other, _) => {
                    others_sender.send(other).ok();
                }
            }
        }
    });
    others
}

async fn forwarded_after(
    service: RemoteService,
    mut others: mpsc::UnboundedReceiver<Command>,
) -> Vec<Command> {
    drop(service);
    let mut forwarded = Vec::new();
    while let Some(command) = tokio::time::timeout(WAIT, others.recv()).await.unwrap() {
        forwarded.push(command);
    }
    forwarded
}

fn token() -> PairingToken {
    PairingToken::random().unwrap()
}

async fn run_until_done(service: RemoteService, requests: Vec<RemoteRequest>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    for request in requests {
        sender.send(request).unwrap();
    }
    drop(sender);
    tokio::time::timeout(WAIT, service.run(receiver, false))
        .await
        .unwrap();
}

#[tokio::test]
async fn controlling_a_device_pauses_this_one_and_shows_the_other() {
    let mut setup = setup().await;
    assert!(setup.service.control(&speaker_id()).await.is_ok());
    assert!(setup.target.is_remote());
    assert!(matches!(
        next_command(&mut setup.commands).await,
        Command::Media(MediaControlEvent::Pause)
    ));
    let shown = tokio::time::timeout(WAIT, setup.shown.recv()).await;
    assert!(matches!(shown, Ok(Some(_))), "the remote state is shown");
    setup.service.release_link(true);
    assert!(!setup.target.is_remote());
    assert!(matches!(
        next_command(&mut setup.commands).await,
        Command::Remote(RemoteCommand::Republish)
    ));
}

#[tokio::test]
async fn moving_the_music_here_adopts_the_other_queue() {
    let mut setup = setup().await;
    assert!(setup.service.control(&speaker_id()).await.is_ok());
    next_command(&mut setup.commands).await;
    setup.service.move_here().await;
    let Command::Remote(RemoteCommand::AdoptTransfer(adopted)) =
        next_command(&mut setup.commands).await
    else {
        panic!("expected the queue to be adopted");
    };
    assert_eq!(adopted.snapshot.items.len(), 2);
    assert_eq!((adopted.position_ms, adopted.is_playing), (7_000, true));
    assert!(!setup.target.is_remote());
}

#[tokio::test]
async fn moving_the_music_there_hands_this_queue_over() {
    let mut setup = setup().await;
    let _others = answer_transfers(setup.commands, Some(Ok(payload_of(3, 12_000))));
    setup.service.move_to(&speaker_id()).await;
    let adopted = setup.speaker.adopted.lock().unwrap().clone();
    assert_eq!(adopted.len(), 1);
    assert_eq!(
        adopted.first().map(|payload| payload.position_ms),
        Some(12_000)
    );
    assert!(setup.target.is_remote());
}

#[tokio::test]
async fn given_a_device_already_controlled_when_controlling_it_again_then_the_link_is_kept() {
    let mut setup = setup().await;
    setup.service.control(&speaker_id()).await.unwrap();
    next_command(&mut setup.commands).await;

    let controlled = setup.service.control(&speaker_id()).await;

    assert!(controlled.is_ok());
    assert!(setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_a_device_that_left_the_network_when_controlling_it_then_nothing_connects() {
    let mut setup = setup().await;

    let controlled = setup.service.control(&DeviceId("gone".into())).await;

    assert!(controlled.is_err());
    assert!(!setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_a_device_that_does_not_listen_when_controlling_it_then_this_player_stays_local() {
    let mut setup = setup().await;
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    setup.service.found = vec![Discovered {
        device: device("radio"),
        address,
    }];

    let controlled = setup.service.control(&DeviceId("radio".into())).await;

    assert!(controlled.is_err());
    assert!(!setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_the_device_refuses_a_control_when_it_answers_then_it_stays_controlled() {
    let mut setup = setup_with(SpeakerDelegate {
        controlling: Err(RemoteFailure::Busy),
        ..SpeakerDelegate::default()
    })
    .await;
    setup.service.control(&speaker_id()).await.unwrap();
    next_command(&mut setup.commands).await;
    setup.target.remote().unwrap().send(RemoteControl::Next);

    let event = tokio::time::timeout(WAIT, next_link_event(&mut setup.service.link))
        .await
        .unwrap();
    let is_refused = matches!(event, LinkEvent::Failed(RemoteFailure::Busy));
    setup.service.on_link_event(event);

    assert!(is_refused);
    assert!(setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_nothing_plays_here_when_moving_the_music_there_then_no_device_is_controlled() {
    let setup = setup().await;
    let others = answer_transfers(setup.commands, Some(Err(RemoteFailure::NothingToTransfer)));
    let mut service = setup.service;

    service.move_to(&speaker_id()).await;

    assert!(!setup.target.is_remote());
    assert!(setup.speaker.adopted.lock().unwrap().is_empty());
    assert!(forwarded_after(service, others).await.is_empty());
}

#[tokio::test]
async fn given_this_player_stopped_when_moving_the_music_there_then_no_device_is_controlled() {
    let setup = setup().await;
    let others = answer_transfers(setup.commands, None);
    let mut service = setup.service;

    service.move_to(&speaker_id()).await;

    assert!(!setup.target.is_remote());
    assert!(forwarded_after(service, others).await.is_empty());
}

#[tokio::test]
async fn given_the_device_left_when_moving_the_music_there_then_this_player_resumes() {
    let setup = setup().await;
    let others = answer_transfers(setup.commands, Some(Ok(payload_of(1, 3_000))));
    let mut service = setup.service;

    service.move_to(&DeviceId("gone".into())).await;

    assert!(!setup.target.is_remote());
    assert!(matches!(
        forwarded_after(service, others).await.as_slice(),
        [Command::Media(MediaControlEvent::Play)]
    ));
}

#[tokio::test]
async fn given_a_paused_player_when_the_move_there_fails_then_it_stays_paused() {
    let setup = setup().await;
    let mut paused = payload_of(1, 3_000);
    paused.is_playing = false;
    let others = answer_transfers(setup.commands, Some(Ok(paused)));
    let mut service = setup.service;

    service.move_to(&DeviceId("gone".into())).await;

    assert!(forwarded_after(service, others).await.is_empty());
}

#[tokio::test]
async fn given_the_device_refuses_the_music_when_moving_it_there_then_this_player_resumes() {
    let setup = setup_with(SpeakerDelegate {
        adopting: Err(RemoteFailure::DifferentServer),
        ..SpeakerDelegate::default()
    })
    .await;
    let others = answer_transfers(setup.commands, Some(Ok(payload_of(2, 3_000))));
    let mut service = setup.service;

    service.move_to(&speaker_id()).await;

    assert_eq!(setup.speaker.adopted.lock().unwrap().len(), 1);
    assert!(matches!(
        forwarded_after(service, others).await.as_slice(),
        [
            Command::Media(MediaControlEvent::Pause),
            Command::Media(MediaControlEvent::Play)
        ]
    ));
}

#[tokio::test]
async fn given_no_device_is_controlled_when_moving_the_music_here_then_nothing_is_adopted() {
    let mut setup = setup().await;

    setup.service.move_here().await;

    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_the_device_has_nothing_to_give_when_moving_the_music_here_then_it_stays_there() {
    let mut setup = setup_with(SpeakerDelegate {
        handing_over: Err(RemoteFailure::NothingToTransfer),
        ..SpeakerDelegate::default()
    })
    .await;
    setup.service.control(&speaker_id()).await.unwrap();
    next_command(&mut setup.commands).await;

    setup.service.move_here().await;

    assert!(setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_the_device_hands_over_an_empty_queue_when_moving_the_music_here_then_it_stays_there()
{
    let mut setup = setup_with(SpeakerDelegate {
        handing_over: Ok(payload_of(0, 0)),
        ..SpeakerDelegate::default()
    })
    .await;
    setup.service.control(&speaker_id()).await.unwrap();
    next_command(&mut setup.commands).await;

    setup.service.move_here().await;

    assert!(setup.target.is_remote());
    assert!(pending_commands(&mut setup.commands).is_empty());
}

#[tokio::test]
async fn given_the_requests_when_the_service_runs_then_it_controls_and_releases_the_device() {
    let mut setup = setup().await;

    run_until_done(
        setup.service,
        vec![
            RemoteRequest::Control(speaker_id()),
            RemoteRequest::ControlLocal,
        ],
    )
    .await;

    assert!(!setup.target.is_remote());
    assert!(matches!(
        pending_commands(&mut setup.commands).as_slice(),
        [
            Command::Media(MediaControlEvent::Pause),
            Command::Remote(RemoteCommand::Republish)
        ]
    ));
}

#[tokio::test]
async fn given_the_requests_when_the_service_runs_then_the_music_moves_there_and_back() {
    let setup = setup().await;
    let mut others = answer_transfers(setup.commands, Some(Ok(payload_of(2, 1_000))));

    run_until_done(
        setup.service,
        vec![
            RemoteRequest::MovePlaybackTo(speaker_id()),
            RemoteRequest::MovePlaybackHere,
        ],
    )
    .await;

    assert_eq!(setup.speaker.adopted.lock().unwrap().len(), 1);
    assert!(!setup.target.is_remote());
    assert!(matches!(
        next_command(&mut others).await,
        Command::Media(MediaControlEvent::Pause)
    ));
    assert!(matches!(
        next_command(&mut others).await,
        Command::Remote(RemoteCommand::AdoptTransfer(adopted)) if adopted.position_ms == 7_000
    ));
}

#[tokio::test]
async fn given_approved_devices_when_forgetting_them_then_they_must_pair_again() {
    let setup = setup().await;
    setup.trust.trust(&device("phone"), token());

    run_until_done(setup.service, vec![RemoteRequest::ForgetDevices]).await;

    assert_eq!(setup.trust.trusted_token(&DeviceId("phone".into())), None);
}

#[tokio::test]
async fn given_a_banned_device_when_unbanning_it_then_it_may_connect_again() {
    let setup = setup().await;
    setup.trust.ban(&device("phone"), IpAddr::from(KITCHEN));

    run_until_done(
        setup.service,
        vec![RemoteRequest::Unban(DeviceId("phone".into()))],
    )
    .await;

    assert!(
        !setup
            .trust
            .is_banned(&DeviceId("phone".into()), IpAddr::from(KITCHEN))
    );
}

#[tokio::test]
async fn given_remote_control_is_off_when_banning_or_disconnecting_then_approvals_are_kept() {
    let setup = setup().await;
    let phone = DeviceId("phone".into());
    let approved = token();
    setup.trust.trust(&device("phone"), approved);

    run_until_done(
        setup.service,
        vec![
            RemoteRequest::Disconnect(phone.clone()),
            RemoteRequest::Ban(phone.clone()),
        ],
    )
    .await;

    assert_eq!(setup.trust.trusted_token(&phone), Some(approved));
    assert!(setup.trust.banned().is_empty());
}

#[tokio::test]
async fn given_finding_players_is_unavailable_when_hosting_is_turned_on_then_nothing_is_served() {
    let mut setup = setup().await;

    setup.service.handle(RemoteRequest::SetHosting(true)).await;

    assert!(setup.service.hosting.is_none());
}

#[tokio::test]
async fn given_a_controlled_device_when_it_drops_the_link_then_this_player_takes_over_again() {
    let mut setup = setup().await;
    let (requests, received) = mpsc::unbounded_channel();
    let running = tokio::spawn(setup.service.run(received, false));
    requests.send(RemoteRequest::Control(speaker_id())).unwrap();
    assert!(matches!(
        next_command(&mut setup.commands).await,
        Command::Media(MediaControlEvent::Pause)
    ));

    setup.speaker_host.disconnect(&DeviceId("laptop".into()));

    assert!(matches!(
        next_command(&mut setup.commands).await,
        Command::Remote(RemoteCommand::Republish)
    ));
    assert!(!setup.target.is_remote());
    drop(requests);
    tokio::time::timeout(WAIT, running).await.unwrap().unwrap();
}

fn hosting_service(service: RemoteService) -> RemoteService {
    let mut hosting = RemoteService::new(service.deps);
    hosting.deps.me = device("test-host");
    hosting
}

#[tokio::test]
async fn given_finding_players_works_when_hosting_is_switched_then_this_player_serves_and_stops() {
    let setup = setup().await;
    let mut service = hosting_service(setup.service);
    assert!(service.discovery.is_some(), "finding players needs mDNS");

    service.handle(RemoteRequest::SetHosting(true)).await;
    service.handle(RemoteRequest::SetHosting(true)).await;
    let is_served = service.hosting.is_some();
    service.handle(RemoteRequest::SetHosting(false)).await;

    assert!(is_served);
    assert!(service.hosting.is_none());
}

#[tokio::test]
async fn given_hosting_when_banning_and_disconnecting_unknown_devices_then_their_approvals_go() {
    let setup = setup().await;
    setup.trust.trust(&device("phone"), token());
    setup.trust.trust(&device("tablet"), token());
    let mut service = hosting_service(setup.service);
    service.handle(RemoteRequest::SetHosting(true)).await;

    service
        .handle(RemoteRequest::Ban(DeviceId("phone".into())))
        .await;
    service
        .handle(RemoteRequest::Disconnect(DeviceId("tablet".into())))
        .await;
    service.handle(RemoteRequest::SetHosting(false)).await;

    assert_eq!(setup.trust.trusted_token(&DeviceId("phone".into())), None);
    assert_eq!(setup.trust.trusted_token(&DeviceId("tablet".into())), None);
}

#[tokio::test]
async fn given_a_player_announced_on_the_network_when_it_is_found_then_it_can_be_controlled() {
    let mut setup = setup().await;
    let announced = std::mem::take(&mut setup.service.found);
    let (announcing, found) = watch::channel(Vec::new());
    let mut devices = Some(found);
    announcing.send(announced).unwrap();

    let discovered = tokio::time::timeout(WAIT, next_devices(&mut devices))
        .await
        .unwrap();
    setup.service.show_devices(discovered);

    assert!(setup.service.control(&speaker_id()).await.is_ok());
    assert!(setup.target.is_remote());
}

#[tokio::test(start_paused = true)]
async fn given_finding_players_stopped_when_waiting_for_players_then_none_ever_arrive() {
    let (announcing, found) = watch::channel(Vec::new());
    let mut devices = Some(found);
    drop(announcing);

    let waited = tokio::time::timeout(WAIT, next_devices(&mut devices)).await;

    assert!(waited.is_err());
    assert!(devices.is_none());
}
