use std::sync::Arc;

use anyhow::{Result, anyhow};
use cadence_api::MusicServer;
use cadence_core::model::ServerConfig;
use slint::{ComponentHandle, SharedString};

use super::loading::load;
use super::view::{SettingsScope, Shared};
use crate::controller::Command;
use crate::ui::{AppState, AppWindow, Context, SettingsState, login, on_ui, show_status};

fn restore(context: &Context, config: &ServerConfig) -> Result<Arc<dyn MusicServer>> {
    let credential = context.store.credential(&config.id)?.ok_or_else(|| {
        anyhow!(tr!(
            "No saved login for {}; remove it and add it again",
            config.name
        ))
    })?;
    context.connector.restore(config, &credential)
}

fn configured_server(shared: &Shared, id: &str) -> Option<ServerConfig> {
    shared
        .view()
        .settings
        .servers
        .iter()
        .find(|server| server.id == id)
        .cloned()
}

pub(super) fn switch_server(scope: &SettingsScope, id: SharedString) {
    let Some(config) = configured_server(scope.shared, id.as_str()) else {
        return;
    };
    scope.window.global::<SettingsState>().set_switching(id);
    let weak_window = scope.window.as_weak();
    let context = scope.context.clone();
    context.runtime.clone().spawn(async move {
        let result = restore_in_background(&context, &config).await;
        if let Ok(server) = &result {
            login::enter_session(&context, server, login::Connection::restored(config));
        }
        on_ui(&weak_window, move |window| {
            show_switch_result(window, result);
        });
    });
}

async fn restore_in_background(
    context: &Context,
    config: &ServerConfig,
) -> Result<Arc<dyn MusicServer>> {
    let (context, config) = (context.clone(), config.clone());
    let restored = tokio::task::spawn_blocking(move || restore(&context, &config)).await;
    restored
        .map_err(anyhow::Error::from)
        .and_then(|restore_result| restore_result)
}

fn show_switch_result(window: &AppWindow, result: Result<Arc<dyn MusicServer>>) {
    let state = window.global::<SettingsState>();
    match result {
        Ok(server) => {
            login::enter_main(window, &server);
            state.invoke_load("".into());
        }
        Err(error) => {
            state.set_switching("".into());
            show_status(
                window,
                &tr!("Could not switch server: {}", format!("{error:#}")),
            );
        }
    }
}

pub(super) fn log_out(window: &AppWindow, context: &Context, shared: &Shared) {
    if let Some(id) = shared.view().settings.current_server_id.take() {
        context.send(Command::RemoveServer(id));
    }
    context.session.set(None);
    window.global::<AppState>().set_server_name("".into());
    login::show_login(window, "");
}

pub(super) fn remove_server(scope: &SettingsScope, id: &SharedString) {
    let SettingsScope {
        window,
        context,
        shared,
    } = *scope;
    let current = shared.view().settings.current_server_id.clone();
    if current.as_deref() == Some(id.as_str()) {
        log_out(window, context, shared);
        return;
    }
    context.send(Command::RemoveServer(id.into()));
    load(window, context, shared);
}
