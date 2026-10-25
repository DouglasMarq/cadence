use std::sync::mpsc as std_mpsc;
use std::time::Instant;

use mpris_server::{Server, Signal};
use tokio::sync::mpsc;

use super::transition::UpdateSignals;
use super::{MprisPlayer, Update};
use crate::{PlatformError, Result};

type ReadySender = std_mpsc::SyncSender<Result<()>>;

pub(super) fn run(
    player: MprisPlayer,
    update_receiver: mpsc::UnboundedReceiver<Update>,
    ready_sender: ReadySender,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    match runtime {
        Ok(runtime) => runtime.block_on(serve(player, update_receiver, ready_sender)),
        Err(error) => {
            report_registration(
                &ready_sender,
                Err(PlatformError::MediaControls(Box::new(error))),
            );
        }
    }
}

async fn serve(
    player: MprisPlayer,
    mut update_receiver: mpsc::UnboundedReceiver<Update>,
    ready_sender: ReadySender,
) {
    let server = match register(player).await {
        Ok(server) => server,
        Err(error) => {
            report_registration(&ready_sender, Err(error));
            return;
        }
    };
    report_registration(&ready_sender, Ok(()));

    while let Some(update) = update_receiver.recv().await {
        publish_update(&server, update).await;
    }
}

fn report_registration(ready_sender: &ReadySender, registration: Result<()>) {
    if ready_sender.send(registration).is_err() {
        tracing::debug!(
            "MPRIS registration result arrived after MediaControls::new stopped waiting"
        );
    }
}

async fn publish_update(server: &Server<MprisPlayer>, update: Update) {
    let signals = {
        let mut snapshot = server.imp().core.snapshot();
        let transition = snapshot.transition_for(update, Instant::now());
        snapshot.apply(transition.change);
        transition.signals
    };
    publish(server, signals).await;
}

async fn publish(server: &Server<MprisPlayer>, signals: UpdateSignals) {
    if !signals.properties.is_empty()
        && let Err(error) = server.properties_changed(signals.properties).await
    {
        tracing::warn!(%error, "MPRIS properties changed signal failed");
    }
    if let Some(position) = signals.seeked_to
        && let Err(error) = server.emit(Signal::Seeked { position }).await
    {
        tracing::warn!(%error, "MPRIS seeked signal failed");
    }
}

async fn register(player: MprisPlayer) -> Result<Server<MprisPlayer>> {
    let suffix = player.core.bus_suffix.clone();
    match Server::new(&suffix, player.clone()).await {
        Ok(server) => Ok(server),
        Err(first_error) => {
            let fallback = format!("{suffix}.instance{}", std::process::id());
            tracing::debug!(%first_error, %fallback, "MPRIS bus name unavailable, using fallback");
            Server::new(&fallback, player)
                .await
                .map_err(|error| PlatformError::MediaControls(Box::new(error)))
        }
    }
}
