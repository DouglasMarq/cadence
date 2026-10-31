use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use cadence_core::model::{Album, Artist, ItemKind, Song};
use cadence_storage::view_cache::{CachedView, ViewCache, ViewKey};
use serde::{Deserialize, Serialize};

use super::snapshot::Overlay;
use super::view::View;

const MAX_REMEMBERED_EDITS: usize = 512;
const LEDGER_ITEM: &str = "ledger";
const LEDGER_TAKEN_AT_MS: u64 = 0;
const AWAITING_SERVER: u64 = u64::MAX;

pub trait Annotated {
    const KIND: ItemKind;
    fn id(&self) -> &str;
    fn set_favorite(&mut self, is_favorite: bool);
    fn set_rating(&mut self, rating: u8);
}

macro_rules! annotated {
    ($item:ty, $kind:expr) => {
        impl Annotated for $item {
            const KIND: ItemKind = $kind;

            fn id(&self) -> &str {
                &self.id
            }

            fn set_favorite(&mut self, is_favorite: bool) {
                self.is_favorite = is_favorite;
            }

            fn set_rating(&mut self, rating: u8) {
                self.user_rating = (rating > 0).then_some(rating);
            }
        }
    };
}

annotated!(Song, ItemKind::Song);
annotated!(Album, ItemKind::Album);
annotated!(Artist, ItemKind::Artist);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamped<Value> {
    value: Value,
    at_ms: u64,
}

impl<Value: Copy> Stamped<Value> {
    fn since(self, taken_at_ms: u64) -> Option<Value> {
        (self.at_ms >= taken_at_ms).then_some(self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ItemEdit {
    kind: ItemKind,
    id: String,
    favorite: Option<Stamped<bool>>,
    rating: Option<Stamped<u8>>,
}

impl ItemEdit {
    fn newest_at_ms(&self) -> u64 {
        let favorite = self.favorite.map_or(0, |favorite| favorite.at_ms);
        let rating = self.rating.map_or(0, |rating| rating.at_ms);
        favorite.max(rating)
    }

    fn stamp(&mut self, at_ms: u64) {
        if let Some(favorite) = &mut self.favorite {
            favorite.at_ms = at_ms;
        }
        if let Some(rating) = &mut self.rating {
            rating.at_ms = at_ms;
        }
    }
}

type ServerEdits = HashMap<(ItemKind, String), ItemEdit>;

pub struct Edits<'ledger> {
    items: &'ledger ServerEdits,
    taken_at_ms: u64,
}

impl Edits<'_> {
    pub fn apply<Item: Annotated>(&self, item: &mut Item) {
        let Some(edit) = self.items.get(&(Item::KIND, item.id().to_owned())) else {
            return;
        };
        if let Some(is_favorite) = edit
            .favorite
            .and_then(|stamp| stamp.since(self.taken_at_ms))
        {
            item.set_favorite(is_favorite);
        }
        if let Some(rating) = edit.rating.and_then(|stamp| stamp.since(self.taken_at_ms)) {
            item.set_rating(rating);
        }
    }
}

#[derive(Default)]
pub struct EditLedger {
    servers: Mutex<HashMap<String, ServerEdits>>,
    forgotten: Mutex<HashMap<(String, String), u64>>,
    restored: tokio::sync::Mutex<HashSet<String>>,
}

fn lock<Guarded>(mutex: &Mutex<Guarded>) -> MutexGuard<'_, Guarded> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn ledger_key(server_id: &str) -> ViewKey {
    View::Edits.slot(LEDGER_ITEM).key(server_id)
}

fn forget_oldest(edits: &mut ServerEdits) {
    let oldest = edits
        .iter()
        .min_by_key(|(_key, edit)| edit.newest_at_ms())
        .map(|(key, _edit)| key.clone());
    if let Some(oldest) = oldest {
        edits.remove(&oldest);
    }
}

fn blank_edit(kind: ItemKind, id: &str) -> ItemEdit {
    ItemEdit {
        kind,
        id: id.to_owned(),
        favorite: None,
        rating: None,
    }
}

impl EditLedger {
    fn with_edit(&self, server_id: &str, item: (ItemKind, &str), edit: impl FnOnce(&mut ItemEdit)) {
        let (kind, id) = item;
        let mut servers = lock(&self.servers);
        let edits = servers.entry(server_id.to_owned()).or_default();
        edit(
            edits
                .entry((kind, id.to_owned()))
                .or_insert_with(|| blank_edit(kind, id)),
        );
        if edits.len() > MAX_REMEMBERED_EDITS {
            forget_oldest(edits);
        }
    }

    pub fn begin_favorite(
        &self,
        server_id: &str,
        (kind, ids): (ItemKind, &[String]),
        is_favorite: bool,
    ) {
        let favorite = Stamped {
            value: is_favorite,
            at_ms: AWAITING_SERVER,
        };
        for id in ids {
            self.with_edit(server_id, (kind, id), |edit| {
                edit.favorite = Some(favorite);
            });
        }
    }

    pub fn begin_rating(&self, server_id: &str, (kind, id): (ItemKind, &str), rating: u8) {
        let rating = Stamped {
            value: rating,
            at_ms: AWAITING_SERVER,
        };
        self.with_edit(server_id, (kind, id), |edit| edit.rating = Some(rating));
    }

    pub fn settle(
        &self,
        server_id: &str,
        items: (ItemKind, &[String]),
        confirmed_at_ms: Option<u64>,
    ) {
        let (kind, ids) = items;
        let mut servers = lock(&self.servers);
        let Some(edits) = servers.get_mut(server_id) else {
            return;
        };
        for id in ids {
            let key = (kind, id.clone());
            match confirmed_at_ms {
                Some(at_ms) => {
                    if let Some(edit) = edits.get_mut(&key) {
                        edit.stamp(at_ms);
                    }
                }
                None => {
                    edits.remove(&key);
                }
            }
        }
    }

    pub fn overlay<Payload: Overlay>(
        &self,
        server_id: &str,
        taken_at_ms: u64,
        payload: &mut Payload,
    ) {
        let servers = lock(&self.servers);
        let Some(items) = servers.get(server_id) else {
            return;
        };
        payload.overlay(&Edits { items, taken_at_ms });
    }

    pub fn mark_forgotten(&self, key: &ViewKey, at_ms: u64) {
        let group = (key.server_id().to_owned(), key.group().to_owned());
        lock(&self.forgotten).insert(group, at_ms);
    }

    pub fn was_forgotten_since(&self, key: &ViewKey, taken_at_ms: u64) -> bool {
        let group = (key.server_id().to_owned(), key.group().to_owned());
        lock(&self.forgotten)
            .get(&group)
            .is_some_and(|forgotten_at_ms| *forgotten_at_ms >= taken_at_ms)
    }

    pub async fn restore(&self, cache: &ViewCache, server_id: &str) {
        let mut restored = self.restored.lock().await;
        if restored.contains(server_id) {
            return;
        }
        let saved = cache.read::<Vec<ItemEdit>>(&ledger_key(server_id)).await;
        if let Some(view) = saved {
            let mut servers = lock(&self.servers);
            let edits = servers.entry(server_id.to_owned()).or_default();
            for edit in view.payload {
                edits.entry((edit.kind, edit.id.clone())).or_insert(edit);
            }
        }
        restored.insert(server_id.to_owned());
    }

    pub async fn persist(&self, cache: &ViewCache, server_id: &str) {
        let payload: Vec<ItemEdit> = lock(&self.servers)
            .get(server_id)
            .map(|edits| edits.values().cloned().collect())
            .unwrap_or_default();
        let view = CachedView {
            taken_at_ms: LEDGER_TAKEN_AT_MS,
            payload,
        };
        cache.write(&ledger_key(server_id), view).await;
    }
}
