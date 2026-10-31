use std::fs::DirEntry;
use std::io::ErrorKind;
use std::path::Path;
use std::time::SystemTime;

use lru::LruCache;

use crate::{Result, TEMPORARY_FILE_PREFIX};

pub(crate) struct DiskIndex {
    files: LruCache<String, u64>,
    total_bytes: u64,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CachedFile {
    modified: SystemTime,
    name: String,
    length_bytes: u64,
}

impl DiskIndex {
    pub(crate) fn scan(directory: &Path) -> Result<Self> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            if let Some(cached_file) = cached_file_from(&entry?)? {
                found.push(cached_file);
            }
        }
        found.sort();
        let mut index = Self {
            files: LruCache::unbounded(),
            total_bytes: 0,
        };
        for cached_file in found {
            index.insert(cached_file.name, cached_file.length_bytes);
        }
        Ok(index)
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.files.contains(name)
    }

    pub(crate) fn promote(&mut self, name: &str) {
        self.files.promote(name);
    }

    pub(crate) fn insert(&mut self, name: String, length_bytes: u64) {
        if let Some(previous_length_bytes) = self.files.put(name, length_bytes) {
            self.total_bytes = self.total_bytes.saturating_sub(previous_length_bytes);
        }
        self.total_bytes = self.total_bytes.saturating_add(length_bytes);
    }

    pub(crate) fn remove(&mut self, directory: &Path, name: &str) {
        if let Some(length_bytes) = self.files.pop(name) {
            self.total_bytes = self.total_bytes.saturating_sub(length_bytes);
        }
        remove_ignoring_errors(&directory.join(name));
    }

    pub(crate) fn names_starting_with(&self, prefix: &str) -> Vec<String> {
        self.files
            .iter()
            .map(|(name, _length_bytes)| name)
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect()
    }

    pub(crate) fn evict(&mut self, directory: &Path, max_bytes: u64) {
        while self.total_bytes > max_bytes {
            let Some((name, length_bytes)) = self.files.pop_lru() else {
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(length_bytes);
            remove_evicted_file(directory, &name);
        }
    }
}

fn cached_file_from(entry: &DirEntry) -> Result<Option<CachedFile>> {
    let metadata = entry.metadata()?;
    if !metadata.is_file() {
        return Ok(None);
    }
    let Ok(name) = entry.file_name().into_string() else {
        return Ok(None);
    };
    if name.starts_with(TEMPORARY_FILE_PREFIX) {
        remove_ignoring_errors(&entry.path());
        return Ok(None);
    }
    Ok(Some(CachedFile {
        modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        name,
        length_bytes: metadata.len(),
    }))
}

fn remove_evicted_file(directory: &Path, name: &str) {
    match std::fs::remove_file(directory.join(name)) {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            tracing::warn!("cache eviction failed for {name}: {error}");
        }
        _ => {}
    }
}

fn remove_ignoring_errors(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        tracing::debug!("could not remove cache file {path:?}: {error}");
    }
}
