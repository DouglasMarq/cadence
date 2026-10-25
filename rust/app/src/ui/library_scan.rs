use std::sync::Arc;
use std::time::Duration;

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::LibraryScan;
use slint::{ComponentHandle, Weak};

use super::{AppState, AppWindow, Context, on_ui, show_status, tracks};
use crate::session::BackgroundWork;

const SCAN_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_SCAN_POLLS: usize = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanOutcome {
    Finished(LibraryScan),
    StillRunning,
}

pub fn install(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let context = context.clone();
    window.global::<AppState>().on_rescan_library(move || {
        if let Some(window) = weak.upgrade() {
            start_scan(&window, &context);
        }
    });
}

fn start_scan(window: &AppWindow, context: &Context) {
    let state = window.global::<AppState>();
    if state.get_is_library_scanning() {
        return;
    }
    let Some(server) = context.session.get() else {
        show_status(window, &tr!("Not connected to a server"));
        return;
    };
    show_progress(window, None);
    let weak = window.as_weak();
    let work = context.session.background_work();
    context.runtime.spawn(async move {
        let outcome = follow_scan_while_connected(work, server, progress_reporter(&weak)).await;
        on_ui(&weak, move |window| match outcome {
            Some(outcome) => finish_scan(window, outcome),
            None => hide_progress(window),
        });
    });
}

fn progress_reporter(weak: &Weak<AppWindow>) -> impl Fn(Option<u64>) + Send + 'static {
    let weak = weak.clone();
    move |scanned_items| {
        on_ui(&weak, move |window| show_progress(window, scanned_items));
    }
}

async fn follow_scan_while_connected(
    work: BackgroundWork,
    server: Arc<dyn MusicServer>,
    report: impl Fn(Option<u64>),
) -> Option<Result<ScanOutcome, ApiError>> {
    work.run(follow_scan(&server, report)).await
}

async fn follow_scan(
    server: &Arc<dyn MusicServer>,
    report: impl Fn(Option<u64>),
) -> Result<ScanOutcome, ApiError> {
    server.start_library_scan().await?;
    for _ in 0..MAX_SCAN_POLLS {
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
        let scan = server.library_scan().await?;
        if !scan.is_scanning {
            return Ok(ScanOutcome::Finished(scan));
        }
        report(scan.scanned_items);
    }
    Ok(ScanOutcome::StillRunning)
}

fn show_progress(window: &AppWindow, scanned_items: Option<u64>) {
    let state = window.global::<AppState>();
    state.set_is_library_scanning(true);
    let progress = scanned_items.map_or_else(String::new, |count| {
        tr_plural!("{n} item so far", "{n} items so far", count)
    });
    state.set_library_scan_progress(progress.into());
}

fn hide_progress(window: &AppWindow) {
    let state = window.global::<AppState>();
    state.set_is_library_scanning(false);
    state.set_library_scan_progress("".into());
}

fn finish_scan(window: &AppWindow, outcome: Result<ScanOutcome, ApiError>) {
    hide_progress(window);
    if matches!(outcome, Ok(ScanOutcome::Finished(_))) {
        tracks::refresh(window);
    }
    show_status(window, &outcome_message(&outcome));
}

fn outcome_message(outcome: &Result<ScanOutcome, ApiError>) -> String {
    match outcome {
        Ok(ScanOutcome::Finished(LibraryScan {
            scanned_items: Some(count),
            ..
        })) => tr_plural!(
            "Library scan finished: {n} item",
            "Library scan finished: {n} items",
            *count
        ),
        Ok(ScanOutcome::Finished(_)) => tr!("Library scan finished"),
        Ok(ScanOutcome::StillRunning) => tr!("The library scan is still running on the server"),
        Err(ApiError::NotAuthorized(_)) => tr!("Only a server admin can rescan the library"),
        Err(error) => tr!("Could not rescan the library: {}", error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::demo::FakeServer;
    use crate::session::Session;
    use crate::tests::refusing_server::RefusingServer;
    use crate::ui::test_window::TestWindow;

    fn is_scanning(view: &TestWindow) -> bool {
        view.app.global::<AppState>().get_is_library_scanning()
    }

    #[test]
    fn given_a_rescan_when_the_server_finishes_then_the_status_reports_the_items() {
        let Some(view) = TestWindow::open(Arc::new(FakeServer::new(30))) else {
            return;
        };
        let state = view.app.global::<AppState>();

        state.invoke_rescan_library();

        assert!(is_scanning(&view));
        assert!(view.pump(|| state.get_library_scan_progress().ends_with("so far")));
        assert!(view.pump(|| !is_scanning(&view)));
        assert_eq!(view.status(), "Library scan finished: 30 items");
        assert_eq!(state.get_library_scan_progress(), "");
    }

    #[test]
    fn given_a_scan_already_running_when_rescanning_then_nothing_new_starts() {
        let Some(view) = TestWindow::open(Arc::new(FakeServer::new(30))) else {
            return;
        };
        let state = view.app.global::<AppState>();
        state.set_is_library_scanning(true);

        state.invoke_rescan_library();

        assert_eq!(state.get_library_scan_progress(), "");
        assert_eq!(view.status(), "");
    }

    #[test]
    fn given_no_server_when_rescanning_then_the_status_says_so() {
        let Some(view) = TestWindow::open(Arc::new(FakeServer::new(30))) else {
            return;
        };
        view.context.session.set(None);

        view.app.global::<AppState>().invoke_rescan_library();

        assert_eq!(view.status(), "Not connected to a server");
        assert!(!is_scanning(&view));
    }

    #[test]
    fn given_a_user_without_admin_rights_when_rescanning_then_the_status_explains_why() {
        let Some(view) = TestWindow::open(Arc::new(RefusingServer::new(30))) else {
            return;
        };

        view.app.global::<AppState>().invoke_rescan_library();

        assert!(view.pump(|| !is_scanning(&view)));
        assert_eq!(view.status(), "Only a server admin can rescan the library");
    }

    #[tokio::test(start_paused = true)]
    async fn given_a_logout_library_scan_polling_stops() {
        let session = Session::default();
        let server: Arc<dyn MusicServer> = Arc::new(FakeServer::new(30));
        session.set(Some(Arc::clone(&server)));
        let reports = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&reports);
        let report = move |scanned_items| recorded.lock().unwrap().push(scanned_items);
        let scan = tokio::spawn(follow_scan_while_connected(
            session.background_work(),
            Arc::clone(&server),
            report,
        ));
        tokio::time::sleep(SCAN_POLL_INTERVAL + SCAN_POLL_INTERVAL / 2).await;
        assert_eq!(reports.lock().unwrap().len(), 1);

        session.set(None);

        assert!(scan.await.unwrap().is_none());
        tokio::time::sleep(SCAN_POLL_INTERVAL * 4).await;
        assert_eq!(reports.lock().unwrap().len(), 1);
    }

    fn finished(scanned_items: Option<u64>) -> Result<ScanOutcome, ApiError> {
        Ok(ScanOutcome::Finished(LibraryScan {
            is_scanning: false,
            scanned_items,
        }))
    }

    #[test]
    fn finished_scans_report_the_item_count_when_known() {
        assert_eq!(
            outcome_message(&finished(Some(42))),
            "Library scan finished: 42 items"
        );
        assert_eq!(outcome_message(&finished(None)), "Library scan finished");
    }

    #[test]
    fn users_without_admin_rights_are_told_why() {
        let refused = Err(ApiError::NotAuthorized("User not authorized".into()));
        assert_eq!(
            outcome_message(&refused),
            "Only a server admin can rescan the library"
        );
    }

    #[test]
    fn other_failures_show_the_error() {
        let failed = Err(ApiError::Network("timed out".into()));
        assert_eq!(
            outcome_message(&failed),
            "Could not rescan the library: network error: timed out"
        );
    }
}
