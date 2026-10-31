use std::sync::Arc;

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::RadioStation;
use serde::{Deserialize, Serialize};
use slint::ComponentHandle;
use tokio::sync::oneshot;

use super::freshness::LoadTicket;
use super::model::RadioModel;
use super::view::RadioView;
use crate::controller::{Command, SettingsSnapshot};
use crate::ui::{AppWindow, RadioState, show_status};
use crate::views::{Overlay, View};

const STATIONS_ITEM: &str = "stations";
const CAN_MANAGE_ITEM: &str = "can-manage";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ServerStations(Vec<RadioStation>);

fn may_carry_credentials(url: &str) -> bool {
    let after_scheme = url.split_once("://").map_or(url, |(_scheme, rest)| rest);
    let authority = after_scheme.split('/').next().unwrap_or_default();
    authority.contains('@') || url.contains('?')
}

fn is_safe_to_store(station: &RadioStation) -> bool {
    let homepage = station.homepage_url.as_deref().unwrap_or_default();
    !may_carry_credentials(&station.stream_url) && !may_carry_credentials(homepage)
}

impl Overlay for ServerStations {
    fn drop_live_fields(&mut self) {
        self.0.retain(is_safe_to_store);
    }
}

async fn saved_favorites(reply: oneshot::Receiver<SettingsSnapshot>) -> Vec<RadioStation> {
    reply
        .await
        .map(|snapshot| snapshot.settings.radio.favorites)
        .inspect_err(|_| tracing::debug!("the controller did not answer with the radio favorites"))
        .unwrap_or_default()
}

impl RadioView {
    pub(super) fn load(&self) {
        self.context.session.open_view();
        if let Some(window) = self.window.upgrade() {
            let state = window.global::<RadioState>();
            state.set_is_stations_loading(true);
        }
        let ticket = self.with_model(RadioModel::begin_load);
        self.load_favorites(ticket);
        match self.context.session.get() {
            Some(server) => {
                self.load_server_stations(&server, ticket);
                self.load_permissions(&server, ticket);
            }
            None => self.show_without_a_server(ticket),
        }
    }

    fn load_favorites(&self, ticket: LoadTicket) {
        let (reply, answer) = oneshot::channel();
        self.context.send(Command::GetSettings(reply));
        let view = self.clone();
        self.context.runtime.spawn(async move {
            let favorites = saved_favorites(answer).await;
            let shown = view.clone();
            view.on_window(move |window| shown.apply_favorites(window, ticket, favorites));
        });
    }

    fn load_server_stations(&self, server: &Arc<dyn MusicServer>, ticket: LoadTicket) {
        let view = self.clone();
        let asked = Arc::clone(server);
        let stations = async move { asked.radio_stations().await.map(ServerStations) };
        let slot = View::Radio.slot(STATIONS_ITEM);
        self.context
            .views(server)
            .load(Some(slot), stations, move |result| {
                let stations = result.map(|stations| stations.0);
                let shown = view.clone();
                view.on_window(move |window| {
                    shown.apply_server_stations(window, ticket, stations);
                });
            });
    }

    fn load_permissions(&self, server: &Arc<dyn MusicServer>, ticket: LoadTicket) {
        let view = self.clone();
        let asked = Arc::clone(server);
        let can_manage = async move { asked.can_manage_radio_stations().await };
        let slot = View::Radio.slot(CAN_MANAGE_ITEM);
        self.context
            .views(server)
            .load(Some(slot), can_manage, move |result| {
                let shown = view.clone();
                view.on_window(move |window| shown.apply_can_manage(window, ticket, result));
            });
    }

    fn show_without_a_server(&self, ticket: LoadTicket) {
        let shown = self.clone();
        self.on_window(move |window| {
            shown.apply_server_stations(window, ticket, Ok(Vec::new()));
            shown.apply_can_manage(window, ticket, Ok(false));
        });
    }

    pub(super) fn apply_favorites(
        &self,
        window: &AppWindow,
        ticket: LoadTicket,
        favorites: Vec<RadioStation>,
    ) {
        self.with_model(|model| model.accept_favorites(ticket, favorites));
        self.render(window);
    }

    pub(super) fn apply_server_stations(
        &self,
        window: &AppWindow,
        ticket: LoadTicket,
        result: cadence_api::Result<Vec<RadioStation>>,
    ) {
        if !self.with_model(|model| model.is_current_load(ticket)) {
            return;
        }
        let has_radio = !matches!(result, Err(ApiError::Unsupported(_)));
        let (stations, failure) = match result {
            Ok(stations) => (stations, None),
            Err(ApiError::Unsupported(_)) => (Vec::new(), None),
            Err(error) => (Vec::new(), Some(error)),
        };
        self.with_model(|model| model.accept_server_stations(ticket, stations));
        if let Some(error) = failure {
            show_status(window, &tr!("Could not load the radio stations: {}", error));
        }
        let state = window.global::<RadioState>();
        state.set_has_server_radio(has_radio);
        state.set_is_stations_loading(false);
        self.render(window);
    }

    pub(super) fn apply_can_manage(
        &self,
        window: &AppWindow,
        ticket: LoadTicket,
        result: cadence_api::Result<bool>,
    ) {
        let can_manage = result
            .inspect_err(|error| tracing::debug!("radio permissions unknown: {error}"))
            .unwrap_or(false);
        self.with_model(|model| model.accept_can_manage(ticket, can_manage));
        self.render(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(stream_url: &str, homepage_url: Option<&str>) -> RadioStation {
        RadioStation {
            id: "station-1".into(),
            name: "Station".into(),
            stream_url: stream_url.into(),
            homepage_url: homepage_url.map(Into::into),
        }
    }

    #[test]
    fn given_a_plain_stream_url_when_stored_then_the_station_is_kept() {
        let mut stations = ServerStations(vec![station(
            "https://radio.example.com/stream",
            Some("https://radio.example.com"),
        )]);
        stations.drop_live_fields();
        assert_eq!(stations.0.len(), 1);
    }

    #[test]
    fn given_a_url_with_embedded_access_when_stored_then_the_station_is_left_out() {
        let mut stations = ServerStations(vec![
            station("https://listener:fictitious@radio.example.com/stream", None),
            station("https://radio.example.com/stream?token=fictitious", None),
            station(
                "https://radio.example.com/stream",
                Some("https://radio.example.com/?session=fictitious"),
            ),
        ]);
        stations.drop_live_fields();
        assert!(stations.0.is_empty());
    }
}
