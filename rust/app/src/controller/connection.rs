use cadence_api::Credential;
use cadence_core::model::ServerConfig;

use super::{Controller, UiUpdate};

impl Controller {
    pub(super) fn connected(&mut self, config: ServerConfig, credential: Option<Credential>) {
        if self.is_switching_server(&config.id) {
            self.clear_queue();
        }
        if let Some(credential) = &credential {
            self.save_credential(&config, credential).ok();
        }
        self.remember_server(config);
        self.save_settings();
        self.persist_credential();
        self.resume_queue_after_connect();
    }

    fn is_switching_server(&self, server_id: &str) -> bool {
        self.settings
            .current_server_id
            .as_ref()
            .is_some_and(|current_id| current_id != server_id)
    }

    fn remember_server(&mut self, config: ServerConfig) {
        self.settings.current_server_id = Some(config.id.clone());
        let servers = &mut self.settings.servers;
        servers.retain(|saved| saved.id != config.id);
        servers.push(config);
    }

    fn resume_queue_after_connect(&mut self) {
        if self.queue.is_empty() {
            self.fetch_server_queue();
            return;
        }
        let Some(track) = self.current_track() else {
            return;
        };
        if !self.is_radio_playing() {
            self.update_media_metadata(&track);
        }
        self.deps
            .ui
            .send(UiUpdate::NowPlaying(Some(Box::new(track))));
    }

    pub(super) fn remove_server(&mut self, server_id: &str) {
        if let Err(error) = self.deps.store.delete_credential(server_id) {
            tracing::warn!("deleting the saved login failed: {error:#}");
            self.deps.ui.send(UiUpdate::Error(tr!(
                "Could not delete the saved login: {}",
                format!("{error:#}")
            )));
        }
        self.settings
            .servers
            .retain(|server| server.id != server_id);
        if self.settings.current_server_id.as_deref() == Some(server_id) {
            self.settings.current_server_id = None;
            self.deps.session.set(None);
            self.clear_queue();
        }
        self.save_settings();
    }
}
