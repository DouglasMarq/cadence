mod entry;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::cover_cache::disk_index::DiskIndex;
use crate::cover_cache::image_file::mark_recently_used;
use crate::cover_cache::locking::{lock, run_blocking};
use crate::{Result, write_atomic};
pub use entry::{CachedView, ViewKey};
use entry::{decode, encode, group_prefix, server_prefix};

pub const VIEW_CACHE_DIRECTORY: &str = "views";
const DEFAULT_MAX_DISK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;

struct CacheState {
    dir: PathBuf,
    max_disk_bytes: u64,
    index: Mutex<Option<DiskIndex>>,
}

pub struct ViewCache {
    state: Arc<CacheState>,
}

impl ViewCache {
    pub fn new(cache_dir: &Path) -> Self {
        Self::bounded(cache_dir.join(VIEW_CACHE_DIRECTORY), DEFAULT_MAX_DISK_BYTES)
    }

    pub fn bounded(dir: PathBuf, max_disk_bytes: u64) -> Self {
        Self {
            state: Arc::new(CacheState {
                dir,
                max_disk_bytes,
                index: Mutex::new(None),
            }),
        }
    }

    pub async fn read<Payload>(&self, key: &ViewKey) -> Option<CachedView<Payload>>
    where
        Payload: DeserializeOwned + Send + 'static,
    {
        let state = Arc::clone(&self.state);
        let key = key.clone();
        run_blocking(move || state.read(&key))
            .await
            .inspect_err(|error| tracing::warn!("view cache read failed: {error}"))
            .ok()
            .flatten()
    }

    pub async fn write<Payload>(&self, key: &ViewKey, view: CachedView<Payload>)
    where
        Payload: Serialize + Send + 'static,
    {
        let state = Arc::clone(&self.state);
        let key = key.clone();
        let outcome = run_blocking(move || state.write(&key, &view)).await;
        if let Err(error) = outcome {
            tracing::warn!("view cache write failed: {error}");
        }
    }

    pub async fn forget_group(&self, key: &ViewKey) {
        self.forget_prefix(group_prefix(key)).await;
    }

    pub async fn forget_server(&self, server_id: &str) {
        self.forget_prefix(server_prefix(server_id)).await;
    }

    pub async fn disk_usage(&self) -> u64 {
        let state = Arc::clone(&self.state);
        run_blocking(move || state.with_index(|index, _dir| index.total_bytes()))
            .await
            .unwrap_or(0)
    }

    async fn forget_prefix(&self, prefix: String) {
        let state = Arc::clone(&self.state);
        let outcome = run_blocking(move || {
            state.with_index(|index, dir| {
                for name in index.names_starting_with(&prefix) {
                    index.remove(dir, &name);
                }
            })
        })
        .await;
        if let Err(error) = outcome {
            tracing::warn!("view cache invalidation failed: {error}");
        }
    }
}

impl CacheState {
    fn with_index<Output>(
        &self,
        action: impl FnOnce(&mut DiskIndex, &Path) -> Output,
    ) -> Result<Output> {
        let mut slot = lock(&self.index);
        let index = match slot.take() {
            Some(index) => index,
            None => self.scan()?,
        };
        Ok(action(slot.insert(index), &self.dir))
    }

    fn scan(&self) -> Result<DiskIndex> {
        std::fs::create_dir_all(&self.dir)?;
        let mut index = DiskIndex::scan(&self.dir)?;
        index.evict(&self.dir, self.max_disk_bytes);
        Ok(index)
    }

    fn read<Payload: DeserializeOwned>(
        &self,
        key: &ViewKey,
    ) -> Result<Option<CachedView<Payload>>> {
        let name = key.file_name();
        if !self.with_index(|index, _dir| index.contains(&name))? {
            return Ok(None);
        }
        let path = self.dir.join(&name);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.drop_entry(&name)?;
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        match decode(&bytes, key) {
            Ok(view) => {
                self.with_index(|index, _dir| index.promote(&name))?;
                mark_recently_used(&path);
                Ok(Some(view))
            }
            Err(reason) => {
                tracing::warn!("ignoring the view cache entry {name}: {reason}");
                self.drop_entry(&name)?;
                Ok(None)
            }
        }
    }

    fn drop_entry(&self, name: &str) -> Result<()> {
        self.with_index(|index, dir| index.remove(dir, name))
    }

    fn write<Payload: Serialize>(&self, key: &ViewKey, view: &CachedView<Payload>) -> Result<()> {
        let bytes = encode(view, key)?;
        if bytes.len() > MAX_ENTRY_BYTES {
            tracing::debug!("not caching a view of {} bytes", bytes.len());
            return self.drop_entry(&key.file_name());
        }
        self.with_index(|_index, _dir| ())?;
        let name = key.file_name();
        write_atomic(&self.dir.join(&name), &bytes)?;
        self.with_index(|index, dir| {
            index.insert(name, bytes.len() as u64);
            index.evict(dir, self.max_disk_bytes);
        })
    }
}

#[cfg(test)]
mod tests;
