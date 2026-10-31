use std::future::Future;
use std::sync::{Arc, PoisonError, RwLock};

use cadence_api::MusicServer;
use cadence_storage::settings::Settings;
use tokio::sync::watch;

#[derive(Clone)]
pub struct Session {
    server: Arc<RwLock<Option<Arc<dyn MusicServer>>>>,
    settings: Arc<RwLock<Arc<Settings>>>,
    work_epoch: Arc<watch::Sender<u64>>,
    view_epoch: Arc<watch::Sender<u64>>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            server: Arc::default(),
            settings: Arc::default(),
            work_epoch: Arc::new(watch::Sender::new(0)),
            view_epoch: Arc::new(watch::Sender::new(0)),
        }
    }
}

fn advance(epoch: &watch::Sender<u64>) {
    epoch.send_modify(|epoch| *epoch = epoch.wrapping_add(1));
}

impl Session {
    pub fn get(&self) -> Option<Arc<dyn MusicServer>> {
        self.server
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, server: Option<Arc<dyn MusicServer>>) {
        *self.server.write().unwrap_or_else(PoisonError::into_inner) = server;
        self.end_background_work();
        self.open_view();
    }

    pub fn settings(&self) -> Arc<Settings> {
        Arc::clone(&self.settings.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn publish_settings(&self, settings: &Settings) {
        *self
            .settings
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Arc::new(settings.clone());
    }

    pub fn end_background_work(&self) {
        advance(&self.work_epoch);
    }

    pub fn background_work(&self) -> BackgroundWork {
        BackgroundWork {
            epoch: self.work_epoch.subscribe(),
        }
    }

    pub fn open_view(&self) {
        advance(&self.view_epoch);
    }

    pub fn view_work(&self) -> BackgroundWork {
        BackgroundWork {
            epoch: self.view_epoch.subscribe(),
        }
    }
}

pub struct BackgroundWork {
    epoch: watch::Receiver<u64>,
}

impl BackgroundWork {
    pub async fn run<Output>(mut self, work: impl Future<Output = Output>) -> Option<Output> {
        tokio::select! {
            output = work => Some(output),
            () = self.ended() => {
                tracing::debug!("stopped work because its session or view ended");
                None
            }
        }
    }

    async fn ended(&mut self) {
        if self.epoch.changed().await.is_err() {
            tracing::debug!("the session is gone, ending its background work");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::demo::FakeServer;

    const COUNT_STEP: Duration = Duration::from_millis(1);
    const WORK_TIME: Duration = Duration::from_millis(5);

    async fn count_forever(steps: &AtomicUsize) {
        loop {
            steps.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(COUNT_STEP).await;
        }
    }

    async fn stops_when(session: &Session, end: impl FnOnce(&Session)) {
        let steps = Arc::new(AtomicUsize::new(0));
        let counting = Arc::clone(&steps);
        let work = session.background_work();
        let task = tokio::spawn(async move { work.run(count_forever(&counting)).await });
        tokio::time::advance(WORK_TIME).await;
        end(session);
        let stopped = tokio::time::timeout(Duration::from_secs(5), task).await;
        assert!(matches!(stopped, Ok(Ok(None))));
        let steps_at_stop = steps.load(Ordering::SeqCst);
        tokio::time::advance(WORK_TIME).await;
        assert_eq!(steps.load(Ordering::SeqCst), steps_at_stop);
    }

    #[tokio::test(start_paused = true)]
    async fn logging_out_stops_background_work() {
        let session = Session::default();
        session.set(Some(Arc::new(FakeServer::new(10))));
        stops_when(&session, |session| session.set(None)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn shutting_down_stops_background_work_but_keeps_the_server() {
        let session = Session::default();
        session.set(Some(Arc::new(FakeServer::new(10))));
        stops_when(&session, Session::end_background_work).await;
        assert!(session.get().is_some());
    }

    #[tokio::test]
    async fn given_work_for_a_view_when_another_view_opens_then_it_stops() {
        let session = Session::default();
        let work = session.view_work();
        session.open_view();
        assert_eq!(work.run(std::future::pending::<u8>()).await, None);
    }

    #[tokio::test]
    async fn given_work_for_a_view_when_the_server_changes_then_it_stops() {
        let session = Session::default();
        let work = session.view_work();
        session.set(None);
        assert_eq!(work.run(std::future::pending::<u8>()).await, None);
    }

    #[tokio::test]
    async fn given_a_view_opening_then_background_work_goes_on() {
        let session = Session::default();
        let work = session.background_work();
        session.open_view();
        assert_eq!(work.run(async { 7 }).await, Some(7));
    }

    #[tokio::test]
    async fn work_that_finishes_first_returns_its_output() {
        let session = Session::default();
        let output = session.background_work().run(async { 7 }).await;
        assert_eq!(output, Some(7));
        let later = session.background_work();
        session.end_background_work();
        assert_eq!(later.run(std::future::pending::<u8>()).await, None);
    }
}
