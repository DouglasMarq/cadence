use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use cadence_api::MusicServer;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter};
use slint::{ComponentHandle, EventLoopError, PlatformError};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::offline_server::OfflineServer;
use crate::connect::DemoConnector;
use crate::controller::Command;
use crate::covers::DemoCovers;
use crate::session::Session;
use crate::store::JsonStore;
use crate::ui::test_window::{
    Jobs, RoutedWindow, SIZE, claim_routed_platform, pump_until, routed_runtime,
};
use crate::ui::{AppState, AppWindow, Context, login};

const PUMP_TIMEOUT: Duration = Duration::from_secs(10);
const VIEW_CACHE_DIRECTORY: &str = "views";

struct GlobalProxy;

impl EventLoopProxy for GlobalProxy {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        Ok(())
    }

    fn invoke_from_event_loop(
        &self,
        event: Box<dyn FnOnce() + Send>,
    ) -> Result<(), EventLoopError> {
        slint::invoke_from_event_loop(event)
    }
}

struct SessionPlatform {
    window: Rc<MinimalSoftwareWindow>,
    has_local_proxy: Rc<Cell<bool>>,
}

impl Platform for SessionPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        self.has_local_proxy
            .get()
            .then(|| Box::new(GlobalProxy) as Box<dyn EventLoopProxy>)
    }
}

fn routed_jobs_of_this_test() -> Option<Jobs> {
    let test_name = std::thread::current().name()?.to_owned();
    std::thread::Builder::new()
        .name(test_name)
        .spawn(|| {
            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
            claim_routed_platform(&window)
        })
        .ok()?
        .join()
        .ok()
        .flatten()
}

fn install_platform(window: &Rc<MinimalSoftwareWindow>) -> Option<Jobs> {
    let jobs = routed_jobs_of_this_test()?;
    let has_local_proxy = Rc::new(Cell::new(false));
    let platform = SessionPlatform {
        window: Rc::clone(window),
        has_local_proxy: Rc::clone(&has_local_proxy),
    };
    slint::platform::set_platform(Box::new(platform)).ok()?;
    has_local_proxy.set(true);
    Some(jobs)
}

struct Host {
    window: Rc<MinimalSoftwareWindow>,
    jobs: Jobs,
    runtime: Runtime,
    dir: tempfile::TempDir,
}

impl Host {
    fn open() -> Self {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        let jobs = install_platform(&window).expect("a Slint platform for this test thread");
        Self {
            window,
            jobs,
            runtime: routed_runtime(2),
            dir: tempfile::tempdir().expect("a temporary store directory"),
        }
    }
}

pub struct SessionWindow {
    pub app: AppWindow,
    pub context: Context,
    commands: RefCell<UnboundedReceiver<Command>>,
    host: Host,
}

impl SessionWindow {
    pub fn open(server: Arc<dyn MusicServer>) -> Self {
        Self::start(Host::open(), server)
    }

    pub fn restart(self, server: Arc<dyn MusicServer>) -> Self {
        let Self { app, host, .. } = self;
        drop(app);
        Self::start(host, server)
    }

    fn start(host: Host, server: Arc<dyn MusicServer>) -> Self {
        let (sender, commands) = mpsc::unbounded_channel();
        let context = Context {
            runtime: host.runtime.handle().clone(),
            commands: sender,
            session: Session::default(),
            store: Arc::new(JsonStore::new(host.dir.path())),
            connector: Arc::new(DemoConnector::default()),
            covers: Arc::new(DemoCovers),
            remote: crate::remote::Remote::default(),
        };
        context.session.set(Some(Arc::clone(&server)));
        let app = AppWindow::new().expect("the app window");
        crate::ui::install(&app, &context, &Default::default());
        app.show().expect("a shown app window");
        host.window.set_size(SIZE);
        login::enter_main(&app, &server);
        Self {
            app,
            context,
            commands: RefCell::new(commands),
            host,
        }
    }

    pub fn pump(&self, done: impl Fn() -> bool) -> bool {
        let routed = RoutedWindow {
            window: &self.host.window,
            jobs: &self.host.jobs,
        };
        pump_until(routed, PUMP_TIMEOUT, done)
    }

    pub fn wait_until_idle(&self) -> bool {
        let metrics = self.context.runtime.metrics();
        self.pump(|| {
            self.drain_commands();
            metrics.num_alive_tasks() == 0
        })
    }

    pub fn cache_directory(&self) -> std::path::PathBuf {
        self.host.dir.path().join(VIEW_CACHE_DIRECTORY)
    }

    pub fn status(&self) -> String {
        self.app.global::<AppState>().get_status().into()
    }

    pub fn pump_until_status(&self, expected: &str) -> bool {
        self.pump(|| self.status().contains(expected))
    }

    pub fn clear_status(&self) {
        self.app.global::<AppState>().set_status("".into());
    }

    pub fn next_command(&self) -> Option<Command> {
        self.commands.borrow_mut().try_recv().ok()
    }

    pub fn drain_commands(&self) {
        while self.next_command().is_some() {}
    }

    pub fn go_offline(&self) {
        self.context
            .session
            .set(Some(Arc::new(OfflineServer::default())));
    }

    pub fn disconnect(&self) {
        self.context.session.set(None);
    }
}
