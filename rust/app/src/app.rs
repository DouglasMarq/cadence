use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use cadence_api::{Credential, MusicServer};
use cadence_core::model::ServerConfig;
use cadence_platform::MediaControlEvent;
use cadence_storage::settings::Settings;
use slint::ComponentHandle;
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, oneshot, watch};

use crate::audio::{AudioOutput, MpvOutput};
use crate::connect::{Connector, DemoConnector, ServerConnector};
use crate::controller::{Command, Controller, Deps, forward_command, load_saved_queue};
use crate::covers::{CachedCovers, CoverProvider, DemoCovers};
use crate::demo::{DemoLibrary, FakeAudio, FakeServer, demo_credential};
use crate::discord::DiscordSession;
use crate::discover::{DemoDiscover, DiscoverSource, YouTubeMusicSource};
use crate::media::{MediaSession, MediaSessions, Mpris, NoMediaSession, SleepInhibition};
use crate::remote::{self, HostFeed, LocalGate, Remote, RemoteRequest, RemoteStart};
use crate::session::Session;
use crate::single_instance::{self, FirstInstance, Launch};
use crate::sleep_timer::{self, SleepTimerUi};
use crate::store::{JsonStore, Store, SystemStore};
use crate::termination;
use crate::tray;
use crate::ui::updates::{self, PendingRelaunch};
use crate::ui::{self, AppWindow, Context, login, on_ui};
use cadence_remote as remote_state;

const SHUTDOWN_SAVE_TIMEOUT: Duration = Duration::from_secs(6);
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const IO_WORKER_THREADS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Real,
    Demo(DemoLibrary),
}

struct Backends {
    store: Arc<dyn Store>,
    connector: Arc<dyn Connector>,
    covers: Arc<dyn CoverProvider>,
    discover: Arc<dyn DiscoverSource>,
}

struct RunningApp<'a> {
    mode: Mode,
    settings: &'a Settings,
    window: &'a AppWindow,
    context: &'a Context,
}

struct Receivers {
    commands: mpsc::UnboundedReceiver<Command>,
    remote: mpsc::UnboundedReceiver<RemoteRequest>,
}

type SavedLogin = (ServerConfig, Result<Option<Credential>>);
type RestoredLogin = Option<(ServerConfig, Arc<dyn MusicServer>)>;

fn backends(mode: Mode) -> Result<Backends> {
    match mode {
        Mode::Demo(library) => Ok(demo_backends(library)),
        Mode::Real => real_backends(),
    }
}

fn demo_backends(library: DemoLibrary) -> Backends {
    Backends {
        store: Arc::new(JsonStore::new(std::env::temp_dir().join("cadence-demo"))),
        connector: Arc::new(DemoConnector::new(library)),
        covers: Arc::new(DemoCovers),
        discover: Arc::new(DemoDiscover::new(library)),
    }
}

fn real_backends() -> Result<Backends> {
    let store = SystemStore::from_env()?;
    let covers = CachedCovers::new(store.paths())?;
    Ok(Backends {
        store: Arc::new(store),
        connector: Arc::new(ServerConnector::new()?),
        covers: Arc::new(covers),
        discover: Arc::new(YouTubeMusicSource::default()),
    })
}

fn audio_output(
    mode: Mode,
    settings: &Settings,
    commands: &mpsc::UnboundedSender<Command>,
) -> Result<Box<dyn AudioOutput>> {
    let commands = commands.clone();
    let on_event = Box::new(move |event| {
        forward_command(&commands, Command::Player(event));
    });
    Ok(match mode {
        Mode::Demo(_) => {
            Box::new(FakeAudio::new(on_event).with_crossfade(settings.playback.crossfade()))
        }
        Mode::Real => Box::new(MpvOutput::new(&settings.playback, on_event)?),
    })
}

fn mpris_session(commands: &mpsc::UnboundedSender<Command>) -> Option<Box<dyn MediaSession>> {
    let commands = commands.clone();
    let on_event = Box::new(move |event| {
        forward_command(&commands, Command::Media(event));
    });
    Mpris::new(on_event)
        .inspect_err(|error| tracing::warn!("MPRIS unavailable: {error:#}"))
        .ok()
        .map(|mpris| Box::new(mpris) as Box<dyn MediaSession>)
}

fn sleep_inhibition() -> Option<Box<dyn MediaSession>> {
    SleepInhibition::new()
        .inspect_err(|error| tracing::warn!("sleep inhibition unavailable: {error:#}"))
        .ok()
        .map(|inhibition| Box::new(inhibition) as Box<dyn MediaSession>)
}

fn discord_client_id(settings: &Settings) -> Option<String> {
    settings
        .integrations
        .discord_client_id
        .as_deref()
        .map(str::trim)
        .filter(|client_id| !client_id.is_empty())
        .map(str::to_owned)
}

fn discord_session(settings: &Settings) -> Option<Box<dyn MediaSession>> {
    let Some(client_id) = discord_client_id(settings) else {
        tracing::warn!("Discord presence needs a discord_client_id in the settings");
        return None;
    };
    DiscordSession::new(client_id)
        .inspect_err(|error| tracing::warn!("Discord presence unavailable: {error:#}"))
        .ok()
        .map(|discord| Box::new(discord) as Box<dyn MediaSession>)
}

fn combine_media_sessions(mut sessions: Vec<Box<dyn MediaSession>>) -> Box<dyn MediaSession> {
    match sessions.len() {
        0 | 1 => sessions.pop().unwrap_or_else(|| Box::new(NoMediaSession)),
        _ => Box::new(MediaSessions(sessions)),
    }
}

fn media_session(
    mode: Mode,
    settings: &Settings,
    commands: &mpsc::UnboundedSender<Command>,
) -> Box<dyn MediaSession> {
    let mut sessions: Vec<Box<dyn MediaSession>> = Vec::new();
    if mode == Mode::Real && settings.integrations.is_mpris_enabled {
        sessions.extend(mpris_session(commands));
    }
    if mode == Mode::Real {
        sessions.extend(sleep_inhibition());
    }
    if settings.integrations.is_discord_rich_presence_enabled {
        sessions.extend(discord_session(settings));
    }
    combine_media_sessions(sessions)
}

fn save_demo_credential(context: &Context) {
    let config = FakeServer::config_for_demo();
    if let Err(error) = context.store.set_credential(&config.id, &demo_credential()) {
        tracing::warn!("saving the demo credential failed: {error:#}");
    }
}

fn demo_login() -> SavedLogin {
    (FakeServer::config_for_demo(), Ok(Some(demo_credential())))
}

fn last_used_login(context: &Context, settings: &Settings) -> Option<SavedLogin> {
    settings
        .current_server_id
        .as_deref()
        .and_then(|server_id| {
            settings
                .servers
                .iter()
                .find(|server| server.id == server_id)
        })
        .map(|config| (config.clone(), context.store.credential(&config.id)))
}

fn saved_login(context: &Context, settings: &Settings, mode: Mode) -> Option<SavedLogin> {
    match mode {
        Mode::Demo(_) => Some(demo_login()),
        Mode::Real => last_used_login(context, settings),
    }
}

fn restored_login(context: &Context, settings: &Settings, mode: Mode) -> Result<RestoredLogin> {
    let Some((config, credential)) = saved_login(context, settings, mode) else {
        return Ok(None);
    };
    let Some(credential) = credential? else {
        return Ok(None);
    };
    let server = context.connector.restore(&config, &credential)?;
    Ok(Some((config, server)))
}

fn show_restored_session(window: &AppWindow, context: &Context, restored: Result<RestoredLogin>) {
    match restored {
        Ok(Some((_, server))) => login::open_main(window, context, &server),
        Ok(None) => login::show_login(window, ""),
        Err(error) => login::show_login(window, &format!("{error:#}")),
    }
}

fn restore_session(app: &RunningApp) {
    let (weak_window, context, settings) = (
        app.window.as_weak(),
        app.context.clone(),
        app.settings.clone(),
    );
    let mode = app.mode;
    let runtime = context.runtime.clone();
    runtime.spawn(async move {
        let reading = context.clone();
        let restored = tokio::task::spawn_blocking(move || {
            if let Mode::Demo(_) = mode {
                save_demo_credential(&reading);
            }
            restored_login(&reading, &settings, mode)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|restored| restored);
        if let Ok(Some((config, server))) = &restored {
            login::enter_session(
                &context,
                server,
                login::Connection::restored(config.clone()),
            );
        }
        on_ui(&weak_window, move |window| {
            show_restored_session(window, &context, restored);
        });
    });
}

fn build_runtime() -> Result<Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(IO_WORKER_THREADS)
        .thread_name("cadence-io")
        .enable_all()
        .build()?)
}

fn load_settings_or_default(store: &dyn Store) -> Settings {
    store.load_settings().unwrap_or_else(|error| {
        tracing::warn!("loading settings failed, using defaults: {error:#}");
        Settings::default()
    })
}

fn restore_saved_queue_once_shown(context: &Context) {
    let runtime = context.runtime.clone();
    let store = Arc::clone(&context.store);
    let commands = context.commands.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        runtime.spawn(load_saved_queue(store, commands));
    });
}

fn app_context(runtime: &Runtime, backends: Backends) -> (Context, Receivers) {
    let (commands, command_receiver) = mpsc::unbounded_channel();
    let (remote_requests, remote_receiver) = mpsc::unbounded_channel();
    let context = Context {
        runtime: runtime.handle().clone(),
        commands,
        session: Session::default(),
        store: backends.store,
        connector: backends.connector,
        covers: backends.covers,
        remote: Remote::new(remote_requests),
    };
    let receivers = Receivers {
        commands: command_receiver,
        remote: remote_receiver,
    };
    (context, receivers)
}

fn controller_deps(
    app: &RunningApp,
    host_state: watch::Sender<remote_state::HostState>,
) -> Result<Deps> {
    let RunningApp {
        mode,
        settings,
        window,
        context,
    } = *app;
    let shown = LocalGate::new(
        ui::player::SlintSink::new(window.as_weak(), context.clone()),
        context.remote.target.clone(),
    );
    Ok(Deps {
        audio: audio_output(mode, settings, &context.commands)?,
        media: media_session(mode, settings, &context.commands),
        store: Arc::clone(&context.store),
        session: context.session.clone(),
        ui: Arc::new(SleepTimerUi::new(
            HostFeed::new(shown, host_state),
            window.as_weak(),
        )),
        commands: context.commands.clone(),
        covers: Arc::clone(&context.covers),
    })
}

fn save_state_on_exit(runtime: &Runtime, app: &RunningApp) {
    let (done, finished) = oneshot::channel();
    app.context.send(Command::Shutdown {
        ui: ui::ui_settings(app.window, &app.settings.ui),
        done,
    });
    runtime.block_on(async {
        if tokio::time::timeout(SHUTDOWN_SAVE_TIMEOUT, finished)
            .await
            .is_err()
        {
            tracing::warn!("timed out saving state on exit");
        }
    });
}

fn install_views(app: &RunningApp, discover: Arc<dyn DiscoverSource>) {
    let RunningApp {
        settings,
        window,
        context,
        ..
    } = *app;
    ui::install(window, context, settings);
    ui::appearance::watch_system_scheme(window, context);
    ui::discover::install(window, context, discover);
    sleep_timer::install(window, context);
    tray::start(window, settings.ui.tray, &context.commands);
}

fn start_remote(
    app: &RunningApp,
    host_state: watch::Receiver<remote_state::HostState>,
    requests: mpsc::UnboundedReceiver<RemoteRequest>,
) {
    let RunningApp {
        mode,
        settings,
        window,
        context,
    } = *app;
    remote::start(
        context,
        RemoteStart {
            mode,
            is_hosting: settings.integrations.is_remote_control_enabled,
            is_asking_skipped: settings.integrations.is_remote_approval_skipped,
            window: window.as_weak(),
            host_state,
            mirror: Arc::new(ui::player::SlintSink::new(
                window.as_weak(),
                context.clone(),
            )),
            requests,
        },
    );
}

fn run_until_quit(window: &AppWindow) -> Result<()> {
    window.show().context("showing the window")?;
    slint::run_event_loop_until_quit().context("running the window")?;
    window.hide().context("hiding the window")
}

fn claim_instance(mode: Mode) -> Option<Launch> {
    if let Mode::Demo(_) = mode {
        return None;
    }
    single_instance::instance_directory()
        .map_err(anyhow::Error::from)
        .and_then(|directory| Ok(single_instance::claim(&directory)?))
        .inspect_err(|error| tracing::warn!("single instance check unavailable: {error:#}"))
        .ok()
}

fn raise_on_later_launch(first: FirstInstance, commands: &mpsc::UnboundedSender<Command>) {
    let commands = commands.clone();
    let raise = move || forward_command(&commands, Command::Media(MediaControlEvent::Raise));
    if let Err(error) = first.on_later_launch(raise) {
        tracing::warn!("listening for later launches failed: {error}");
    }
}

fn start_updates(app: &RunningApp) -> PendingRelaunch {
    match app.mode {
        Mode::Demo(_) => PendingRelaunch::default(),
        Mode::Real => updates::start(app.window, app.context),
    }
}

pub fn run(mode: Mode) -> Result<()> {
    let first = match claim_instance(mode) {
        Some(Launch::AlreadyRunning) => return Ok(()),
        Some(Launch::First(first)) => Some(first),
        None => None,
    };
    let runtime = build_runtime()?;
    let backends = backends(mode)?;
    let settings = load_settings_or_default(backends.store.as_ref());

    let window = crate::window_chrome::create_window()?;
    crate::i18n::select_language(settings.ui.language.as_deref());
    crate::identity::announce_xdg_app_id();
    let (host_state, host_state_receiver) = watch::channel(remote_state::HostState::default());
    let discover = Arc::clone(&backends.discover);
    let (context, receivers) = app_context(&runtime, backends);
    let app = RunningApp {
        mode,
        settings: &settings,
        window: &window,
        context: &context,
    };
    install_views(&app, discover);
    if let Some(first) = first {
        raise_on_later_launch(first, &context.commands);
    }

    let deps = controller_deps(&app, host_state)?;
    let mut controller = Controller::new(deps, settings.clone(), None, None);
    controller.expect_saved_queue();
    runtime.spawn(controller.run(receivers.commands));
    start_remote(&app, host_state_receiver, receivers.remote);

    restore_session(&app);
    restore_saved_queue_once_shown(&context);
    termination::quit_on_termination_signals(runtime.handle());
    let relaunch = start_updates(&app);
    run_until_quit(&window)?;

    save_state_on_exit(&runtime, &app);
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_TIMEOUT);
    relaunch.run_after_exit_of(std::process::id());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_with(store: Arc<JsonStore>) -> Context {
        let (commands, _ignored) = mpsc::unbounded_channel();
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

    fn with_discord_client_id(client_id: Option<&str>) -> Settings {
        let mut settings = Settings::default();
        settings.integrations.discord_client_id = client_id.map(str::to_owned);
        settings
    }

    #[test]
    fn given_no_discord_client_id_when_starting_the_presence_then_it_has_none_to_connect_with() {
        assert_eq!(discord_client_id(&with_discord_client_id(None)), None);
        assert_eq!(discord_client_id(&with_discord_client_id(Some("  "))), None);
    }

    #[test]
    fn given_a_discord_client_id_when_starting_the_presence_then_it_connects_with_it_trimmed() {
        let settings = with_discord_client_id(Some(" 123 "));
        assert_eq!(discord_client_id(&settings).as_deref(), Some("123"));
    }

    fn last_used_demo_server() -> Settings {
        let config = FakeServer::config_for_demo();
        Settings {
            current_server_id: Some(config.id.clone()),
            servers: vec![config],
            ..Settings::default()
        }
    }

    #[tokio::test]
    async fn given_a_saved_login_of_the_last_server_the_session_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonStore::new(directory.path()));
        store.set_credential("demo", &demo_credential()).unwrap();
        let context = context_with(store);

        let restored = restored_login(&context, &last_used_demo_server(), Mode::Real).unwrap();

        let (config, server) = restored.unwrap();
        assert_eq!(config, FakeServer::config_for_demo());
        assert_eq!(server.config().id, "demo");
    }

    #[tokio::test]
    async fn given_no_saved_login_nothing_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let context = context_with(Arc::new(JsonStore::new(directory.path())));

        let restored = restored_login(&context, &last_used_demo_server(), Mode::Real).unwrap();

        assert!(restored.is_none());
        let without_server = restored_login(&context, &Settings::default(), Mode::Real).unwrap();
        assert!(without_server.is_none());
    }
}
