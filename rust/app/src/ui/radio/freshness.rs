use cadence_core::model::RadioStation;

use super::model::RadioModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LoadTicket {
    generation: u64,
    favorites_revision: u64,
}

impl RadioModel {
    pub(super) fn begin_load(&mut self) -> LoadTicket {
        self.load_generation += 1;
        LoadTicket {
            generation: self.load_generation,
            favorites_revision: self.favorites_revision,
        }
    }

    pub(super) fn is_current_load(&self, ticket: LoadTicket) -> bool {
        ticket.generation == self.load_generation
    }

    pub(super) fn accept_favorites(&mut self, ticket: LoadTicket, favorites: Vec<RadioStation>) {
        if self.is_current_load(ticket) && ticket.favorites_revision == self.favorites_revision {
            self.favorites = favorites;
        }
    }

    pub(super) fn accept_server_stations(
        &mut self,
        ticket: LoadTicket,
        stations: Vec<RadioStation>,
    ) {
        if self.is_current_load(ticket) {
            self.server_stations = stations;
        }
    }

    pub(super) fn accept_can_manage(&mut self, ticket: LoadTicket, can_manage: bool) {
        if self.is_current_load(ticket) {
            self.can_manage = can_manage;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(id: &str) -> RadioStation {
        RadioStation {
            id: id.into(),
            stream_url: format!("https://{id}.example.com/stream"),
            ..RadioStation::default()
        }
    }

    fn stations(ids: &[&str]) -> Vec<RadioStation> {
        ids.iter().map(|id| station(id)).collect()
    }

    fn accept(
        model: &mut RadioModel,
        ticket: LoadTicket,
        (favorite_ids, server_ids): (&[&str], &[&str]),
    ) {
        model.accept_favorites(ticket, stations(favorite_ids));
        model.accept_server_stations(ticket, stations(server_ids));
        model.accept_can_manage(ticket, true);
    }

    fn ids(stations: &[RadioStation]) -> Vec<&str> {
        stations.iter().map(|saved| saved.id.as_str()).collect()
    }

    #[test]
    fn an_older_load_never_overwrites_a_newer_one() {
        let mut model = RadioModel::default();
        let older = model.begin_load();
        let newer = model.begin_load();
        accept(&mut model, newer, (&["new"], &["1"]));
        assert!(!model.is_current_load(older));
        accept(&mut model, older, (&["old"], &[]));
        assert_eq!(ids(&model.favorites), ["new"]);
        assert_eq!(ids(&model.server_stations), ["1"]);
    }

    #[test]
    fn favorites_changed_during_a_load_are_kept() {
        let mut model = RadioModel::default();
        let ticket = model.begin_load();
        model.remember_favorite(station("added"));
        assert!(model.is_current_load(ticket));
        accept(&mut model, ticket, (&[], &["1"]));
        assert_eq!(ids(&model.favorites), ["added"]);
        assert_eq!(ids(&model.server_stations), ["1"]);
        assert!(model.can_manage);
    }
}
