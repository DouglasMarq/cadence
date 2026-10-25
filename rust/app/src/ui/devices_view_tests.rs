use std::net::IpAddr;
use std::sync::Arc;

use cadence_remote::{Controller, DeviceId, DeviceInfo};
use slint::{ComponentHandle, Model};
use tokio::sync::mpsc;

use crate::controller::Command;
use crate::demo::FakeServer;
use crate::remote::{BannedDevice, Remote, RemoteRequest};
use crate::ui::devices::{
    show_approval, show_banned, show_connecting, show_controllers, show_devices, show_target,
};
use crate::ui::test_window::TestWindow;
use crate::ui::{DevicesState, SettingsState};

const KITCHEN: [u8; 4] = [192, 168, 0, 7];

fn kitchen() -> DeviceInfo {
    DeviceInfo {
        id: DeviceId("kitchen".into()),
        name: "Kitchen".into(),
    }
}

#[test]
fn given_a_pairing_request_the_dialog_shows_the_device_address() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };
    let state = view.app.global::<DevicesState>();

    show_approval(&view.app, Some(("Kitchen", IpAddr::from([192, 168, 0, 7]))));

    assert!(state.get_is_approval_open());
    assert_eq!(state.get_approval_name(), "Kitchen");
    assert_eq!(state.get_approval_address(), "192.168.0.7");

    show_approval(&view.app, None);

    assert!(!state.get_is_approval_open());
    assert_eq!(state.get_approval_address(), "");
}

#[test]
fn given_a_controller_the_devices_button_shows_it() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };
    let state = view.app.global::<DevicesState>();
    let controller = Controller {
        device: kitchen(),
        address: IpAddr::from(KITCHEN),
    };

    show_controllers(&view.app, &[controller]);

    assert_eq!(state.get_controller_names(), "Kitchen");
    let row = state.get_controllers().row_data(0).unwrap();
    assert_eq!(
        (row.id.as_str(), row.address.as_str()),
        ("kitchen", "192.168.0.7")
    );

    show_controllers(&view.app, &[]);

    assert_eq!(state.get_controller_names(), "");
    assert_eq!(state.get_controllers().row_count(), 0);
}

#[test]
fn given_banned_devices_the_settings_list_them() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };
    let banned = BannedDevice {
        id: DeviceId("kitchen".into()),
        name: "Kitchen".into(),
        address: IpAddr::from(KITCHEN),
    };

    show_banned(&view.app, &[banned]);

    let rows = view.app.global::<DevicesState>().get_banned();
    assert_eq!(rows.row_count(), 1);
    assert_eq!(rows.row_data(0).unwrap().name, "Kitchen");
}

#[test]
fn given_the_device_actions_they_send_their_requests() {
    let (requests, mut received) = mpsc::unbounded_channel();
    let Some(view) =
        TestWindow::open_with_remote(Arc::new(FakeServer::new(10)), Remote::new(requests))
    else {
        return;
    };
    let state = view.app.global::<DevicesState>();

    state.invoke_disconnect_device("kitchen".into());
    state.invoke_ban_device("kitchen".into());
    state.invoke_unban_device("kitchen".into());

    let sent: Vec<RemoteRequest> = std::iter::from_fn(|| received.try_recv().ok()).collect();
    let kitchen = || DeviceId("kitchen".into());
    assert_eq!(
        sent,
        [
            RemoteRequest::Disconnect(kitchen()),
            RemoteRequest::Ban(kitchen()),
            RemoteRequest::Unban(kitchen()),
        ]
    );
}

#[test]
fn given_allow_without_asking_is_turned_on_it_is_saved() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };
    let state = view.app.global::<SettingsState>();

    state.set_is_remote_approval_skipped(true);
    state.invoke_integrations_changed();

    let saved = std::iter::from_fn(|| view.next_command()).find_map(|command| match command {
        Command::SaveIntegrations(integrations) => Some(integrations.is_remote_approval_skipped),
        _ => None,
    });
    assert_eq!(saved, Some(true));
}

#[test]
fn given_the_control_actions_they_send_their_requests() {
    let (requests, mut received) = mpsc::unbounded_channel();
    let Some(view) =
        TestWindow::open_with_remote(Arc::new(FakeServer::new(10)), Remote::new(requests))
    else {
        return;
    };
    let state = view.app.global::<DevicesState>();

    state.invoke_control_device("kitchen".into());
    state.invoke_move_playback_to("kitchen".into());
    state.invoke_control_local();
    state.invoke_move_playback_here();

    let sent: Vec<RemoteRequest> = std::iter::from_fn(|| received.try_recv().ok()).collect();
    let kitchen = || DeviceId("kitchen".into());
    assert_eq!(
        sent,
        [
            RemoteRequest::Control(kitchen()),
            RemoteRequest::MovePlaybackTo(kitchen()),
            RemoteRequest::ControlLocal,
            RemoteRequest::MovePlaybackHere,
        ]
    );
}

#[test]
fn given_known_devices_and_a_controlled_one_the_devices_menu_shows_them() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };
    let state = view.app.global::<DevicesState>();

    show_devices(&view.app, &[kitchen()]);
    show_target(&view.app, Some(&kitchen()));
    show_connecting(&view.app, true);

    let row = state.get_devices().row_data(0).unwrap();
    assert_eq!((row.id.as_str(), row.name.as_str()), ("kitchen", "Kitchen"));
    assert!(state.get_is_controlling());
    assert_eq!(
        (
            state.get_active_id().as_str(),
            state.get_active_name().as_str()
        ),
        ("kitchen", "Kitchen")
    );
    assert!(state.get_is_connecting());

    show_target(&view.app, None);

    assert!(!state.get_is_controlling());
    assert_eq!(state.get_active_name(), "");
}

fn answer_pairing(view: &TestWindow, answer: impl Fn(&DevicesState)) -> bool {
    let approvals = view.context.remote.approvals.clone();
    let weak = view.app.as_weak();
    let asking = view.context.runtime.spawn(async move {
        approvals
            .ask(&weak, &kitchen(), IpAddr::from(KITCHEN))
            .await
    });
    let state = view.app.global::<DevicesState>();
    assert!(view.pump(|| state.get_is_approval_open()));
    answer(&state);
    assert!(view.pump(|| asking.is_finished()));
    view.context.runtime.block_on(asking).unwrap_or_default()
}

#[test]
fn given_a_pairing_request_when_allowed_then_the_device_is_let_in() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };

    assert!(answer_pairing(&view, |state| state.invoke_allow_device()));
}

#[test]
fn given_a_pairing_request_when_denied_then_the_device_is_kept_out() {
    let Some(view) = TestWindow::open(Arc::new(FakeServer::new(10))) else {
        return;
    };

    assert!(!answer_pairing(&view, |state| state.invoke_deny_device()));
}
