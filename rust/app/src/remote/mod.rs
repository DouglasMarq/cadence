mod approval;
mod delegate;
mod gate;
mod host_feed;
mod link;
mod lists;
mod messages;
mod service;
mod target;
mod transfer;
mod trust;

use std::sync::Arc;

use cadence_remote::{DeviceId, DeviceInfo, HostState, default_device_name};
use tokio::sync::{mpsc, watch};

pub use approval::Approvals;
pub use delegate::AppHostDelegate;
pub use gate::LocalGate;
pub use host_feed::HostFeed;
pub use target::{PlayerTarget, Routed};
pub use trust::BannedDevice;

use crate::app::Mode;
use crate::controller::UiSink;
use crate::ui::{AppWindow, Context};
use service::{RemoteService, ServiceDeps};
use trust::FileTrust;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteRequest {
    Control(DeviceId),
    ControlLocal,
    MovePlaybackTo(DeviceId),
    MovePlaybackHere,
    SetHosting(bool),
    ForgetDevices,
    Disconnect(DeviceId),
    Ban(DeviceId),
    Unban(DeviceId),
}

#[derive(Clone, Default)]
pub struct Remote {
    pub target: PlayerTarget,
    pub approvals: Approvals,
    requests: Option<mpsc::UnboundedSender<RemoteRequest>>,
}

impl Remote {
    pub fn new(requests: mpsc::UnboundedSender<RemoteRequest>) -> Self {
        Self {
            requests: Some(requests),
            ..Self::default()
        }
    }

    pub fn request(&self, request: RemoteRequest) {
        let sent = self
            .requests
            .as_ref()
            .is_some_and(|requests| requests.send(request).is_ok());
        if !sent {
            tracing::debug!("the remote control service is not running");
        }
    }
}

pub struct RemoteStart {
    pub mode: Mode,
    pub is_hosting: bool,
    pub is_asking_skipped: bool,
    pub window: slint::Weak<AppWindow>,
    pub host_state: watch::Receiver<HostState>,
    pub mirror: Arc<dyn UiSink>,
    pub requests: mpsc::UnboundedReceiver<RemoteRequest>,
}

pub fn start(context: &Context, start: RemoteStart) {
    let (trust, name) = match start.mode {
        Mode::Real => (
            FileTrust::load(Arc::clone(&context.store)),
            default_device_name(),
        ),
        Mode::Demo(_) => (
            FileTrust::ephemeral(Arc::clone(&context.store)),
            tr!("{} (demo)", default_device_name()),
        ),
    };
    context
        .remote
        .approvals
        .skip_asking(start.is_asking_skipped);
    let trust = Arc::new(trust);
    let me = DeviceInfo {
        id: trust.device_id(),
        name,
    };
    let delegate = Arc::new(AppHostDelegate {
        commands: context.commands.clone(),
        session: context.session.clone(),
        approvals: context.remote.approvals.clone(),
        window: start.window.clone(),
    });
    let service = RemoteService::new(ServiceDeps {
        commands: context.commands.clone(),
        session: context.session.clone(),
        window: start.window,
        trust,
        me,
        target: context.remote.target.clone(),
        mirror: start.mirror,
        host_state: start.host_state,
        delegate,
    });
    context
        .runtime
        .spawn(service.run(start.requests, start.is_hosting));
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use cadence_remote::DeviceId;

    use super::*;
    use crate::connect::DemoConnector;
    use crate::controller::UiUpdate;
    use crate::covers::DemoCovers;
    use crate::demo::DemoLibrary;
    use crate::session::Session;
    use crate::store::{JsonStore, StateFile, Store};

    struct Unseen;

    impl UiSink for Unseen {
        fn send(&self, _update: UiUpdate) {}
    }

    fn context_of(store: Arc<dyn Store>) -> Context {
        let (commands, _forwarded) = mpsc::unbounded_channel();
        Context {
            runtime: tokio::runtime::Handle::current(),
            commands,
            session: Session::default(),
            store,
            connector: Arc::new(DemoConnector::default()),
            covers: Arc::new(DemoCovers),
            remote: Remote::default(),
        }
    }

    fn start_of(mode: Mode) -> RemoteStart {
        let (_state, host_state) = watch::channel(HostState::default());
        let (_requests, requests) = mpsc::unbounded_channel();
        RemoteStart {
            mode,
            is_hosting: false,
            is_asking_skipped: true,
            window: slint::Weak::default(),
            host_state,
            mirror: Arc::new(Unseen),
            requests,
        }
    }

    fn phone() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId("phone".into()),
            name: "Phone".into(),
        }
    }

    #[test]
    fn given_requests_when_they_are_made_then_only_a_running_service_receives_them() {
        let (requests, mut received) = mpsc::unbounded_channel();
        let remote = Remote::new(requests);

        remote.request(RemoteRequest::SetHosting(true));
        let first = received.try_recv().ok();
        received.close();
        remote.request(RemoteRequest::ForgetDevices);
        Remote::default().request(RemoteRequest::ForgetDevices);

        assert_eq!(first, Some(RemoteRequest::SetHosting(true)));
        assert_eq!(received.try_recv().ok(), None);
    }

    #[tokio::test]
    async fn given_a_real_start_when_the_service_starts_then_its_identity_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        let store: Arc<dyn Store> = Arc::new(JsonStore::new(directory.path()));
        let context = context_of(Arc::clone(&store));

        start(&context, start_of(Mode::Real));

        assert!(
            store
                .read_state(StateFile::RemoteDevices)
                .unwrap()
                .is_some()
        );
        let is_approved = context
            .remote
            .approvals
            .ask(
                &slint::Weak::default(),
                &phone(),
                IpAddr::from([10, 0, 0, 4]),
            )
            .await;
        assert!(is_approved);
    }

    #[tokio::test]
    async fn given_a_demo_start_when_the_service_starts_then_nothing_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        let store: Arc<dyn Store> = Arc::new(JsonStore::new(directory.path()));
        let context = context_of(Arc::clone(&store));

        start(&context, start_of(Mode::Demo(DemoLibrary::default())));

        assert!(
            store
                .read_state(StateFile::RemoteDevices)
                .unwrap()
                .is_none()
        );
    }
}
