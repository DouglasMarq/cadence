use std::sync::Arc;

use cadence_youtube::music::{CollectionPage, MusicShelf};
use slint::ComponentHandle;

use super::deps::DiscoverView;
use super::model::with_discover;
use super::render::render_sections;
use crate::discover::mixes::{MixDay, MixDelivery, MixId, MixJob, MixSet};
use crate::session::Session;
use crate::ui::{AppWindow, on_ui};

#[derive(Default)]
pub struct MixState {
    set: Option<MixSet>,
    is_building: bool,
    revision: u64,
}

impl MixState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn shelf(&self) -> Option<MusicShelf> {
        self.set.as_ref()?.shelf()
    }

    pub fn page(&self, id: MixId) -> Option<CollectionPage> {
        self.set.as_ref()?.page(id)
    }

    fn shown_set(&self) -> Option<MixSet> {
        self.set.clone()
    }

    fn claim_build(&mut self, server_id: &str, today: MixDay) -> bool {
        let is_fresh = self
            .set
            .as_ref()
            .is_some_and(|set| set.is_fresh_for(server_id, today));
        if self.is_building || is_fresh {
            return false;
        }
        if self
            .set
            .as_ref()
            .is_some_and(|set| !set.is_for_server(server_id))
        {
            self.show(None);
        }
        self.is_building = true;
        true
    }

    fn show(&mut self, set: Option<MixSet>) {
        if self.set == set {
            return;
        }
        self.set = set;
        self.revision += 1;
    }

    fn finish_build(&mut self, set: Option<MixSet>) {
        self.is_building = false;
        self.show(set);
    }
}

pub fn mix_page(window: &AppWindow, id: MixId) -> Option<CollectionPage> {
    with_discover(window, |model| model.mixes.borrow().page(id)).flatten()
}

pub fn ensure_mixes(DiscoverView { window, deps }: DiscoverView) {
    let Some(server) = deps.context.session.get() else {
        return;
    };
    let server_id = server.config().id.clone();
    let today = MixDay::today();
    let is_claimed = with_discover(window, |model| {
        model.mixes.borrow_mut().claim_build(&server_id, today)
    });
    if is_claimed != Some(true) {
        return;
    }
    let shown = with_discover(window, |model| model.mixes.borrow().shown_set()).flatten();
    let job = MixJob {
        server,
        source: Arc::clone(&deps.source),
        store: Arc::clone(&deps.context.store),
        session: deps.context.session.clone(),
        matcher: deps.matcher.clone(),
        today,
    };
    let (weak_window, session) = (window.as_weak(), deps.context.session.clone());
    deps.context.runtime.spawn(job.run(shown, move |delivery| {
        deliver(&weak_window, &session, delivery);
    }));
}

fn deliver(weak_window: &slint::Weak<AppWindow>, session: &Session, delivery: MixDelivery) {
    let session = session.clone();
    on_ui(weak_window, move |window| {
        let current_server = session.get().map(|server| server.config().id.clone());
        let is_current = |set: &MixSet| Some(&set.server_id) == current_server.as_ref();
        with_discover(window, |model| {
            let mut state = model.mixes.borrow_mut();
            match delivery {
                MixDelivery::Saved(mixes) => state.show(mixes.filter(is_current)),
                MixDelivery::Finished(mixes) => state.finish_build(mixes.filter(is_current)),
            }
        });
        render_sections(window);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::mixes::MixSet;

    fn mixes_for(server_id: &str, day: MixDay) -> MixSet {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "server_id": server_id,
            "day": day.0,
            "daily": [],
            "weekly": null
        }))
        .unwrap()
    }

    #[test]
    fn a_build_starts_once_and_not_while_the_mixes_are_fresh() {
        let mut state = MixState::default();
        assert!(state.claim_build("server", MixDay(5)));
        assert_eq!(state.shown_set(), None);
        assert!(!state.claim_build("server", MixDay(5)), "already building");
        state.finish_build(Some(mixes_for("server", MixDay(5))));
        assert_eq!(state.revision(), 1);
        assert!(!state.claim_build("server", MixDay(5)), "fresh");
        assert!(state.claim_build("server", MixDay(6)), "a new day");
    }

    #[test]
    fn mixes_of_another_server_are_hidden_before_rebuilding() {
        let mut state = MixState::default();
        state.finish_build(Some(mixes_for("old", MixDay(5))));
        assert!(state.claim_build("new", MixDay(5)));
        assert_eq!(state.shown_set(), None);
        assert_eq!(state.revision(), 2);
    }
}
