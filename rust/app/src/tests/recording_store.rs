use std::sync::Arc;

use anyhow::Result;
use cadence_api::Credential;
use cadence_storage::queue_store::{EncodedQueue, PlaybackState, StoredQueue};
use cadence_storage::settings::Settings;
use tokio::sync::mpsc;

use crate::store::{JsonStore, StateFile, Store};
use crate::views::ViewStore;

pub(super) struct RecordingStore {
    inner: Arc<JsonStore>,
    saved: mpsc::UnboundedSender<()>,
}

impl RecordingStore {
    pub(super) fn new(inner: Arc<JsonStore>) -> (Self, mpsc::UnboundedReceiver<()>) {
        let (saved, saves) = mpsc::unbounded_channel();
        (Self { inner, saved }, saves)
    }

    fn report_saved(&self, saved: Result<()>) -> Result<()> {
        self.saved.send(()).ok();
        saved
    }
}

impl Store for RecordingStore {
    fn load_settings(&self) -> Result<Settings> {
        self.inner.load_settings()
    }
    fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.inner.save_settings(settings)
    }
    fn load_queue(&self) -> Result<Option<StoredQueue>> {
        self.inner.load_queue()
    }
    fn save_queue(&self, queue: &EncodedQueue) -> Result<()> {
        self.report_saved(self.inner.save_queue(queue))
    }
    fn save_playback(&self, playback: &PlaybackState) -> Result<()> {
        self.report_saved(self.inner.save_playback(playback))
    }
    fn credential(&self, server_id: &str) -> Result<Option<Credential>> {
        self.inner.credential(server_id)
    }
    fn set_credential(&self, server_id: &str, credential: &Credential) -> Result<()> {
        self.inner.set_credential(server_id, credential)
    }
    fn delete_credential(&self, server_id: &str) -> Result<()> {
        self.inner.delete_credential(server_id)
    }
    fn read_state(&self, file: StateFile) -> Result<Option<Vec<u8>>> {
        self.inner.read_state(file)
    }
    fn write_state(&self, file: StateFile, bytes: &[u8]) -> Result<()> {
        self.inner.write_state(file, bytes)
    }
    fn views(&self) -> Arc<ViewStore> {
        self.inner.views()
    }
}
