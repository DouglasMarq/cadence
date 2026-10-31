use std::sync::{Arc, OnceLock};
use std::time::Duration;

use cadence_platform::{MediaControlEvent, open_url};
use cadence_updater::{Offer, Progress, Relaunch, Updater};
use slint::ComponentHandle;

use super::{AppWindow, Context, UpdateStage, UpdateState, on_ui, show_status};
use crate::controller::{Command, forward_command};

const CHECK_DELAY: Duration = Duration::from_secs(5);

pub type OpenPage = Arc<dyn Fn(&str) -> cadence_platform::Result<()> + Send + Sync>;

#[derive(Clone, Default)]
pub struct PendingRelaunch(Arc<OnceLock<Relaunch>>);

impl PendingRelaunch {
    pub fn run_after_exit_of(&self, process_id: u32) {
        let Some(relaunch) = self.0.get() else {
            return;
        };
        if let Err(error) = relaunch.after_exit_of(process_id) {
            tracing::warn!("relaunching after the update failed: {error}");
        }
    }
}

#[derive(Clone)]
pub struct UpdateFlow {
    pub context: Context,
    pub updater: Arc<Updater>,
    pub open_page: OpenPage,
    pub relaunch: PendingRelaunch,
}

pub fn start(window: &AppWindow, context: &Context) -> PendingRelaunch {
    let relaunch = PendingRelaunch::default();
    let weak = window.as_weak();
    let (running, pending) = (context.clone(), relaunch.clone());
    context.runtime.spawn(async move {
        tokio::time::sleep(CHECK_DELAY).await;
        match Updater::for_this_install().await {
            Ok(updater) => {
                let flow = UpdateFlow {
                    context: running,
                    updater: Arc::new(updater),
                    open_page: Arc::new(open_url),
                    relaunch: pending,
                };
                offer_if_available(weak, flow).await;
            }
            Err(error) => tracing::debug!("the updater is unavailable: {error}"),
        }
    });
    relaunch
}

pub async fn offer_if_available(weak: slint::Weak<AppWindow>, flow: UpdateFlow) {
    match flow.updater.check().await {
        Ok(Some(offer)) => on_ui(&weak, move |window| show_offer(window, flow, offer)),
        Ok(None) => {}
        Err(error) => tracing::debug!("checking for updates failed: {error}"),
    }
}

fn show_offer(window: &AppWindow, flow: UpdateFlow, offer: Offer) {
    let state = window.global::<UpdateState>();
    state.set_version(offer.version.to_string().into());
    state.set_can_install(flow.updater.can_install());
    install_callbacks(window, flow, Arc::new(offer));
    state.set_stage(UpdateStage::Offered);
}

fn install_callbacks(window: &AppWindow, flow: UpdateFlow, offer: Arc<Offer>) {
    let state = window.global::<UpdateState>();
    let weak = window.as_weak();
    let later_window = weak.clone();
    state.on_later(move || hide_prompt(&later_window));
    let (opening_window, opening_flow, opening_offer) =
        (weak.clone(), flow.clone(), Arc::clone(&offer));
    state.on_open_release_page(move || {
        hide_prompt(&opening_window);
        open_release_page(
            opening_window.clone(),
            opening_flow.clone(),
            opening_offer.page_url.clone(),
        );
    });
    state.on_update_and_restart(move || {
        if let Some(window) = weak.upgrade() {
            show_progress(&window, Progress::Downloading(0));
        }
        begin_install(weak.clone(), flow.clone(), Arc::clone(&offer));
    });
}

fn hide_prompt(weak: &slint::Weak<AppWindow>) {
    if let Some(window) = weak.upgrade() {
        window
            .global::<UpdateState>()
            .set_stage(UpdateStage::Hidden);
    }
}

fn show_progress(window: &AppWindow, progress: Progress) {
    let state = window.global::<UpdateState>();
    match progress {
        Progress::Downloading(percent) => {
            state.set_stage(UpdateStage::Downloading);
            state.set_percent(i32::from(percent));
        }
        Progress::Installing => state.set_stage(UpdateStage::Installing),
    }
}

fn open_release_page(weak: slint::Weak<AppWindow>, flow: UpdateFlow, url: String) {
    let runtime = flow.context.runtime.clone();
    runtime.spawn_blocking(move || {
        let Err(error) = (flow.open_page)(&url) else {
            return;
        };
        tracing::warn!("opening {url} failed: {error}");
        on_ui(&weak, move |window| {
            show_status(window, &tr!("Could not open the release page: {}", error));
        });
    });
}

fn begin_install(weak: slint::Weak<AppWindow>, flow: UpdateFlow, offer: Arc<Offer>) {
    let runtime = flow.context.runtime.clone();
    runtime.spawn(async move {
        let reporting_window = weak.clone();
        let report = move |progress| {
            on_ui(&reporting_window, move |window| {
                show_progress(window, progress);
            });
        };
        match flow.updater.install(&offer, report).await {
            Ok(relaunch) => quit_and_relaunch(&flow, relaunch),
            Err(error) => {
                tracing::warn!("updating failed: {error}");
                on_ui(&weak, move |window| show_failure(window, &error));
            }
        }
    });
}

fn quit_and_relaunch(flow: &UpdateFlow, relaunch: Relaunch) {
    if flow.relaunch.0.set(relaunch).is_err() {
        tracing::debug!("a relaunch was already requested");
    }
    forward_command(
        &flow.context.commands,
        Command::Media(MediaControlEvent::Quit),
    );
}

fn show_failure(window: &AppWindow, error: &cadence_updater::UpdateError) {
    window
        .global::<UpdateState>()
        .set_stage(UpdateStage::Hidden);
    show_status(window, &tr!("Update failed: {}", error));
}
