use std::sync::Arc;

use anyhow::Result;
use cadence_api::{Credential, MusicServer};
use cadence_core::model::{ServerConfig, ServerType};
use cadence_core::web_url::trimmed_base_url;
use slint::ComponentHandle;

use super::{
    AppState, AppWindow, Context, LoginState, Page, ServerKind, on_ui, sharing, time_based_id,
    tracks,
};
use crate::controller::Command;

fn server_type(kind: ServerKind) -> ServerType {
    match kind {
        ServerKind::Navidrome => ServerType::Navidrome,
        ServerKind::Subsonic => ServerType::Subsonic,
        ServerKind::Jellyfin => ServerType::Jellyfin,
    }
}

pub fn normalize_url(input: &str) -> Option<String> {
    let url = trimmed_base_url(input);
    match (url.is_empty(), url.contains("://")) {
        (true, _) => None,
        (false, true) => Some(url.to_owned()),
        (false, false) => Some(format!("https://{url}")),
    }
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest).to_owned()
}

pub struct Connection {
    pub config: ServerConfig,
    pub credential: Option<Credential>,
}

impl Connection {
    pub fn restored(config: ServerConfig) -> Self {
        Self {
            config,
            credential: None,
        }
    }
}

pub fn enter_session(context: &Context, server: &Arc<dyn MusicServer>, connection: Connection) {
    context.session.set(Some(Arc::clone(server)));
    context.send(Command::Connected {
        config: connection.config,
        credential: connection.credential,
    });
}

pub fn enter_main(window: &AppWindow, server: &Arc<dyn MusicServer>) {
    let app = window.global::<AppState>();
    app.invoke_reset_history();
    app.set_server_name(server.config().name.as_str().into());
    sharing::show_availability(window, server.as_ref());
    app.set_page(Page::Main);
    tracks::refresh(window);
}

pub fn open_main(window: &AppWindow, context: &Context, server: &Arc<dyn MusicServer>) {
    enter_main(window, server);
    watch_session(window, context, server);
}

pub fn watch_session(window: &AppWindow, context: &Context, server: &Arc<dyn MusicServer>) {
    let weak = window.as_weak();
    let context = context.clone();
    let server = Arc::clone(server);
    let work = context.session.background_work();
    context.runtime.clone().spawn(async move {
        if work.run(server.session_lost()).await.is_some() {
            on_ui(&weak, move |window| {
                leave_lost_session(window, &context, &server);
            });
        }
    });
}

fn leave_lost_session(window: &AppWindow, context: &Context, server: &Arc<dyn MusicServer>) {
    let is_current = context
        .session
        .get()
        .is_some_and(|current| Arc::ptr_eq(&current, server));
    if !is_current {
        return;
    }
    context.send(Command::RemoveServer(server.config().id.clone()));
    context.session.set(None);
    window.global::<AppState>().set_server_name("".into());
    show_login(window, &tr!("Your session expired. Log in again."));
}

pub fn show_login(window: &AppWindow, error: &str) {
    window.global::<LoginState>().set_error(error.into());
    window.global::<AppState>().set_page(Page::Login);
}

fn finish(
    window: &slint::Weak<AppWindow>,
    context: &Context,
    result: Result<Arc<dyn MusicServer>>,
) {
    let context = context.clone();
    on_ui(window, move |window| {
        let login = window.global::<LoginState>();
        login.set_is_busy(false);
        match result {
            Ok(server) => {
                login.set_password("".into());
                login.set_error("".into());
                open_main(window, &context, &server);
            }
            Err(error) => login.set_error(format!("{error:#}").into()),
        }
    });
}

async fn login(
    context: &Context,
    config: ServerConfig,
    password: String,
) -> Result<Arc<dyn MusicServer>> {
    let (server, credential) = context.connector.login(config.clone(), &password).await?;
    let connection = Connection {
        credential: Some(credential),
        ..Connection::restored(config)
    };
    enter_session(context, &server, connection);
    Ok(server)
}

fn config_from_form(state: &LoginState<'_>, url: String) -> ServerConfig {
    ServerConfig {
        id: time_based_id(),
        name: host_of(&url),
        server_type: server_type(state.get_server_type()),
        url,
        username: state.get_username().trim().to_owned(),
    }
}

fn install_login(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let login_context = context.clone();
    window.global::<LoginState>().on_login(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<LoginState>();
        let Some(url) = normalize_url(&state.get_url()) else {
            state.set_error(tr!("Enter the server URL").into());
            return;
        };
        let config = config_from_form(&state, url);
        let password = state.get_password().to_string();
        state.set_is_busy(true);
        state.set_error("".into());
        let context = login_context.clone();
        let weak = weak.clone();
        login_context.runtime.spawn(async move {
            let result = login(&context, config, password).await;
            finish(&weak, &context, result);
        });
    });
}

pub fn install(window: &AppWindow, context: &Context) {
    install_login(window, context);
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::connect::DemoConnector;
    use crate::covers::DemoCovers;
    use crate::demo::{FakeServer, demo_credential};
    use crate::session::Session;
    use crate::store::{JsonStore, Store};
    use crate::ui::test_window::TestWindow;

    #[test]
    fn given_each_server_kind_in_the_form_it_maps_to_the_same_server_type() {
        assert_eq!(server_type(ServerKind::Navidrome), ServerType::Navidrome);
        assert_eq!(server_type(ServerKind::Subsonic), ServerType::Subsonic);
        assert_eq!(server_type(ServerKind::Jellyfin), ServerType::Jellyfin);
    }

    #[tokio::test]
    async fn given_a_login_the_credential_travels_to_the_controller() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonStore::new(directory.path()));
        let (commands, mut received) = mpsc::unbounded_channel();
        let context = Context {
            runtime: tokio::runtime::Handle::current(),
            commands,
            session: Session::default(),
            store: store.clone(),
            connector: Arc::new(DemoConnector::default()),
            covers: Arc::new(DemoCovers),
            remote: crate::remote::Remote::default(),
        };
        let config = FakeServer::config_for_demo();

        login(&context, config.clone(), "secret".into())
            .await
            .unwrap();

        assert_eq!(store.credential(&config.id).unwrap(), None);
        let Some(Command::Connected { credential, .. }) = received.recv().await else {
            panic!("expected Connected");
        };
        assert_eq!(credential, Some(demo_credential()));
        assert!(context.session.get().is_some());
    }

    #[tokio::test]
    async fn given_an_entered_session_the_server_is_current_and_the_controller_learns_it() {
        let directory = tempfile::tempdir().unwrap();
        let (commands, mut received) = mpsc::unbounded_channel();
        let context = Context {
            runtime: tokio::runtime::Handle::current(),
            commands,
            session: Session::default(),
            store: Arc::new(JsonStore::new(directory.path())),
            connector: Arc::new(DemoConnector::default()),
            covers: Arc::new(DemoCovers),
            remote: crate::remote::Remote::default(),
        };
        let server: Arc<dyn MusicServer> = Arc::new(FakeServer::new(3));
        let config = FakeServer::config_for_demo();

        enter_session(&context, &server, Connection::restored(config.clone()));

        assert!(
            context
                .session
                .get()
                .is_some_and(|current| current.config().id == config.id)
        );
        let Some(Command::Connected {
            config: connected,
            credential,
        }) = received.recv().await
        else {
            panic!("expected Connected");
        };
        assert_eq!((connected, credential), (config, None));
    }

    #[test]
    fn given_a_lost_session_when_the_app_is_open_then_it_logs_out_and_asks_for_the_login() {
        let server = Arc::new(FakeServer::new(3));
        let dyn_server: Arc<dyn MusicServer> = Arc::clone(&server) as Arc<dyn MusicServer>;
        let Some(view) = TestWindow::open(Arc::clone(&dyn_server)) else {
            return;
        };
        watch_session(&view.app, &view.context, &dyn_server);

        server.lose_session();

        assert!(view.pump(|| view.app.global::<AppState>().get_page() == Page::Login));
        assert_eq!(
            view.app.global::<LoginState>().get_error(),
            "Your session expired. Log in again."
        );
        assert!(view.context.session.get().is_none());
        assert_eq!(view.app.global::<AppState>().get_server_name(), "");
        let commands = std::iter::from_fn(|| view.next_command());
        let removed_ids: Vec<String> = commands
            .filter_map(|command| match command {
                Command::RemoveServer(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(removed_ids, [server.config().id.clone()]);
    }

    #[test]
    fn given_another_server_is_open_when_the_old_one_loses_its_session_then_nothing_changes() {
        let server = Arc::new(FakeServer::new(3));
        let dyn_server: Arc<dyn MusicServer> = Arc::clone(&server) as Arc<dyn MusicServer>;
        let Some(view) = TestWindow::open(Arc::new(FakeServer::new(4))) else {
            return;
        };

        leave_lost_session(&view.app, &view.context, &dyn_server);

        assert_eq!(view.app.global::<AppState>().get_page(), Page::Main);
        assert!(view.context.session.get().is_some());
    }

    fn open_login() -> Option<TestWindow> {
        let view = TestWindow::open(Arc::new(FakeServer::new(3)))?;
        show_login(&view.app, "");
        Some(view)
    }

    fn is_on_main(view: &TestWindow) -> bool {
        view.app.global::<AppState>().get_page() == Page::Main
    }

    #[test]
    fn given_a_server_address_when_logging_in_then_the_library_opens_under_its_host() {
        let Some(view) = open_login() else {
            return;
        };
        let state = view.app.global::<LoginState>();
        state.set_server_type(ServerKind::Jellyfin);
        state.set_url(" music.example.com/ ".into());
        state.set_username(" ada ".into());
        state.set_password("secret".into());

        state.invoke_login();

        assert!(view.pump(|| is_on_main(&view)));
        assert!(!state.get_is_busy());
        assert_eq!(state.get_password(), "");
        assert_eq!(
            view.app.global::<AppState>().get_server_name(),
            "music.example.com"
        );
        let connected =
            std::iter::from_fn(|| view.next_command()).find_map(|command| match command {
                Command::Connected { config, .. } => Some(config),
                _ => None,
            });
        let Some(config) = connected else {
            panic!("expected Connected");
        };
        assert_eq!(
            (
                config.server_type,
                config.url.as_str(),
                config.username.as_str()
            ),
            (ServerType::Jellyfin, "https://music.example.com", "ada")
        );
    }

    #[test]
    fn given_no_server_address_when_logging_in_then_the_form_asks_for_one() {
        let Some(view) = open_login() else {
            return;
        };
        let state = view.app.global::<LoginState>();
        state.set_url("   ".into());

        state.invoke_login();

        assert_eq!(state.get_error(), "Enter the server URL");
        assert!(!state.get_is_busy());
        assert_eq!(view.app.global::<AppState>().get_page(), Page::Login);
    }

    #[test]
    fn urls_are_normalized() {
        assert_eq!(
            normalize_url(" music.example.com/ ").as_deref(),
            Some("https://music.example.com")
        );
        assert_eq!(
            normalize_url("http://10.0.0.2:4533").as_deref(),
            Some("http://10.0.0.2:4533")
        );
        assert_eq!(normalize_url("  "), None);
        assert_eq!(host_of("http://10.0.0.2:4533/nd"), "10.0.0.2:4533");
    }
}
