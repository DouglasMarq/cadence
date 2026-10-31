mod covers;
mod edits;
mod lists;
mod snapshot;
mod view;

#[cfg(test)]
mod tests;

use std::fmt::Display;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use cadence_api::{MusicServer, Result as ApiResult};
use cadence_core::model::ItemKind;
use cadence_core::unix_time::since_unix_epoch;
use cadence_storage::view_cache::{CachedView, ViewCache};
use tokio::runtime::Handle;

use crate::session::Session;
pub use covers::FocusedCovers;
use edits::EditLedger;
pub use lists::{count_slot, page_slot, shelf_slot};
pub use snapshot::{Overlay, Snapshot};
pub use view::{View, ViewSlot};

fn now_ms() -> u64 {
    u64::try_from(since_unix_epoch().as_millis()).unwrap_or(u64::MAX)
}

pub struct ViewStore {
    cache: ViewCache,
    edits: EditLedger,
}

impl ViewStore {
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            cache: ViewCache::new(cache_dir),
            edits: EditLedger::default(),
        }
    }

    pub async fn forget_server(&self, server_id: &str) {
        self.cache.forget_server(server_id).await;
    }

    #[cfg(test)]
    pub async fn seed<Payload: Snapshot>(
        &self,
        server_id: &str,
        slot: &ViewSlot,
        payload: Payload,
    ) {
        let view = CachedView {
            taken_at_ms: now_ms(),
            payload,
        };
        self.cache.write(&slot.key(server_id), view).await;
    }

    #[cfg(test)]
    pub async fn stored<Payload: Snapshot>(
        &self,
        server_id: &str,
        slot: &ViewSlot,
    ) -> Option<Payload> {
        let view = self.cache.read(&slot.key(server_id)).await?;
        Some(view.payload)
    }
}

#[derive(Clone)]
pub struct Views {
    server: Arc<dyn MusicServer>,
    store: Arc<ViewStore>,
    session: Session,
    runtime: Handle,
}

impl Views {
    pub fn new(
        server: Arc<dyn MusicServer>,
        store: Arc<ViewStore>,
        session: Session,
        runtime: Handle,
    ) -> Self {
        Self {
            server,
            store,
            session,
            runtime,
        }
    }

    fn server_id(&self) -> &str {
        &self.server.config().id
    }

    pub fn server(&self) -> &dyn MusicServer {
        self.server.as_ref()
    }

    pub fn load<Payload, Failure, Fetch, Deliver>(
        &self,
        slot: Option<ViewSlot>,
        fetch: Fetch,
        deliver: Deliver,
    ) where
        Payload: Snapshot,
        Failure: Display + Send + 'static,
        Fetch: Future<Output = Result<Payload, Failure>> + Send + 'static,
        Deliver: Fn(Result<Payload, Failure>) + Send + Sync + 'static,
    {
        let refresh = self.clone().revalidate(slot, fetch, deliver);
        let scope = self.session.view_work();
        self.runtime.spawn(async move {
            scope.run(refresh).await;
        });
    }

    pub fn keep_loading<Payload, Failure, Fetch, Deliver>(
        &self,
        slot: Option<ViewSlot>,
        fetch: Fetch,
        deliver: Deliver,
    ) where
        Payload: Snapshot,
        Failure: Display + Send + 'static,
        Fetch: Future<Output = Result<Payload, Failure>> + Send + 'static,
        Deliver: Fn(Result<Payload, Failure>) + Send + Sync + 'static,
    {
        let refresh = self.clone().revalidate(slot, fetch, deliver);
        let scope = self.session.background_work();
        self.runtime.spawn(async move {
            scope.run(refresh).await;
        });
    }

    pub fn load_detached<Payload, Failure, Fetch, Deliver>(
        &self,
        slot: Option<ViewSlot>,
        fetch: Fetch,
        deliver: Deliver,
    ) where
        Payload: Snapshot,
        Failure: Display + Send + 'static,
        Fetch: Future<Output = Result<Payload, Failure>> + Send + 'static,
        Deliver: Fn(Result<Payload, Failure>) + Send + Sync + 'static,
    {
        self.runtime
            .spawn(self.clone().revalidate(slot, fetch, deliver));
    }

    async fn revalidate<Payload, Failure, Fetch, Deliver>(
        self,
        slot: Option<ViewSlot>,
        fetch: Fetch,
        deliver: Deliver,
    ) where
        Payload: Snapshot,
        Failure: Display + Send + 'static,
        Fetch: Future<Output = Result<Payload, Failure>> + Send + 'static,
        Deliver: Fn(Result<Payload, Failure>) + Send + Sync + 'static,
    {
        let cached = self.read_cached::<Payload>(slot.as_ref()).await;
        if let Some(payload) = &cached {
            deliver(Ok(payload.clone()));
        }
        let taken_at_ms = now_ms();
        match fetch.await {
            Ok(mut payload) => {
                self.overlay(taken_at_ms, &mut payload);
                if cached.as_ref() != Some(&payload) {
                    deliver(Ok(payload.clone()));
                }
                payload.drop_live_fields();
                self.save(
                    slot,
                    CachedView {
                        taken_at_ms,
                        payload,
                    },
                );
            }
            Err(error) if cached.is_some() => {
                tracing::warn!("keeping the cached view after a failed refresh: {error}");
            }
            Err(error) => deliver(Err(error)),
        }
    }

    async fn read_cached<Payload: Snapshot>(&self, slot: Option<&ViewSlot>) -> Option<Payload> {
        let key = slot?.key(self.server_id());
        self.store
            .edits
            .restore(&self.store.cache, self.server_id())
            .await;
        let CachedView {
            taken_at_ms,
            mut payload,
        } = self.store.cache.read(&key).await?;
        self.overlay(taken_at_ms, &mut payload);
        Some(payload)
    }

    pub fn taken_now() -> u64 {
        now_ms()
    }

    pub fn overlay<Payload: Overlay>(&self, taken_at_ms: u64, payload: &mut Payload) {
        self.store
            .edits
            .overlay(self.server_id(), taken_at_ms, payload);
    }

    fn save<Payload: Snapshot>(&self, slot: Option<ViewSlot>, view: CachedView<Payload>) {
        let Some(slot) = slot else {
            return;
        };
        let views = self.clone();
        self.runtime.spawn(async move {
            let key = slot.key(views.server_id());
            let is_outdated = views
                .store
                .edits
                .was_forgotten_since(&key, view.taken_at_ms);
            if !is_outdated {
                views.store.cache.write(&key, view).await;
            }
        });
    }

    pub async fn invalidate(&self, view: View) {
        let key = view.slot("").key(self.server_id());
        self.store.edits.mark_forgotten(&key, now_ms());
        self.store.cache.forget_group(&key).await;
    }

    pub async fn invalidate_playlist(&self, playlist_id: &str) {
        self.invalidate(View::Playlist(playlist_id.to_owned()))
            .await;
        self.invalidate(View::Playlists).await;
    }

    pub async fn set_favorite(
        &self,
        kind: ItemKind,
        ids: &[String],
        is_favorite: bool,
    ) -> ApiResult<()> {
        let server_id = self.server_id();
        self.store
            .edits
            .begin_favorite(server_id, (kind, ids), is_favorite);
        let outcome = self.server.set_favorite(kind, ids, is_favorite).await;
        self.settle_edit(kind, ids, outcome.is_ok()).await;
        self.invalidate(View::Favorites).await;
        outcome
    }

    pub async fn set_rating(&self, kind: ItemKind, id: &str, rating: u8) -> ApiResult<()> {
        let server_id = self.server_id();
        let ids = [id.to_owned()];
        self.store.edits.begin_rating(server_id, (kind, id), rating);
        let outcome = self.server.set_rating(kind, id, rating).await;
        self.settle_edit(kind, &ids, outcome.is_ok()).await;
        outcome
    }

    async fn settle_edit(&self, kind: ItemKind, ids: &[String], is_applied: bool) {
        let server_id = self.server_id();
        self.store
            .edits
            .settle(server_id, (kind, ids), is_applied.then(now_ms));
        self.store.edits.persist(&self.store.cache, server_id).await;
    }
}
