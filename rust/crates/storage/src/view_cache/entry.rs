use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Result, StorageError};

const FORMAT_VERSION: u32 = 1;
const NAME_PART_BYTES: usize = 8;
const VIEW_CACHE_ENTRY: &str = "view cache entry";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewKey {
    server_id: String,
    group: String,
    item: String,
}

impl ViewKey {
    pub fn new(server_id: &str, group: &str, item: &str) -> Self {
        Self {
            server_id: server_id.to_owned(),
            group: group.to_owned(),
            item: item.to_owned(),
        }
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn group(&self) -> &str {
        &self.group
    }

    pub(super) fn file_name(&self) -> String {
        format!("{}-{}", group_prefix(self), name_part(&self.item))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedView<Payload> {
    pub taken_at_ms: u64,
    pub payload: Payload,
}

#[derive(Serialize)]
struct StoredEntry<'view, Payload> {
    format: u32,
    server_id: &'view str,
    group: &'view str,
    item: &'view str,
    taken_at_ms: u64,
    payload: &'view Payload,
}

#[derive(Deserialize)]
struct ReadEntry<Payload> {
    format: u32,
    server_id: String,
    group: String,
    item: String,
    taken_at_ms: u64,
    payload: Payload,
}

fn name_part(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(NAME_PART_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn server_prefix(server_id: &str) -> String {
    format!("{}-", name_part(server_id))
}

pub(super) fn group_prefix(key: &ViewKey) -> String {
    format!("{}{}", server_prefix(&key.server_id), name_part(&key.group))
}

pub(super) fn encode<Payload: Serialize>(
    view: &CachedView<Payload>,
    key: &ViewKey,
) -> Result<Vec<u8>> {
    let entry = StoredEntry {
        format: FORMAT_VERSION,
        server_id: &key.server_id,
        group: &key.group,
        item: &key.item,
        taken_at_ms: view.taken_at_ms,
        payload: &view.payload,
    };
    serde_json::to_vec(&entry).map_err(|error| StorageError::malformed(VIEW_CACHE_ENTRY, error))
}

pub(super) fn decode<Payload: DeserializeOwned>(
    bytes: &[u8],
    key: &ViewKey,
) -> Result<CachedView<Payload>> {
    let entry: ReadEntry<Payload> = serde_json::from_slice(bytes)
        .map_err(|error| StorageError::malformed(VIEW_CACHE_ENTRY, error))?;
    if entry.format != FORMAT_VERSION {
        return Err(StorageError::Invalid(format!(
            "format {} is not {FORMAT_VERSION}",
            entry.format
        )));
    }
    let is_same_view =
        entry.server_id == key.server_id && entry.group == key.group && entry.item == key.item;
    if !is_same_view {
        return Err(StorageError::Invalid(
            "the entry belongs to another view".to_owned(),
        ));
    }
    Ok(CachedView {
        taken_at_ms: entry.taken_at_ms,
        payload: entry.payload,
    })
}
