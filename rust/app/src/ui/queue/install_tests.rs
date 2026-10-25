use std::sync::Arc;

use cadence_core::queue::{Queue, Uid};
use slint::{ComponentHandle, Model};

use super::{set_view, shared_view};
use crate::controller::Command;
use crate::demo::{FakeServer, test_song};
use crate::tests::session_window::SessionWindow;
use crate::ui::QueueState;

fn queue_of(count: usize) -> Queue {
    let mut queue = Queue::with_seed(3);
    let songs = (0..count)
        .map(|index| test_song(&format!("s{index}"), 61_000))
        .collect();
    queue.play_now(songs, Some("s1"));
    queue
}

fn uid_at(queue: &Queue, position: usize) -> Uid {
    queue
        .view()
        .item_at(position)
        .map_or(Uid::MAX, |item| item.uid)
}

fn open_with_queue(queue: &Queue) -> SessionWindow {
    let view = SessionWindow::open(Arc::new(FakeServer::new(10)));
    set_view(&view.app, queue.view());
    view.drain_commands();
    view
}

fn state(view: &SessionWindow) -> QueueState<'_> {
    view.app.global::<QueueState>()
}

#[test]
fn given_a_shown_queue_when_its_view_is_set_then_rows_current_position_and_shared_view_follow() {
    let queue = queue_of(4);

    let view = open_with_queue(&queue);

    let queue_state = state(&view);
    assert_eq!(queue_state.get_rows().row_count(), 4);
    assert_eq!(queue_state.get_current_position(), 1);
    assert_eq!(queue_state.get_current_uid(), uid_at(&queue, 1).to_string());
    assert_eq!(shared_view(&view.app).borrow().len(), 4);
}

#[test]
fn given_a_shown_queue_when_a_row_is_played_or_removed_then_the_controller_is_told() {
    let queue = queue_of(3);
    let view = open_with_queue(&queue);
    let uid = uid_at(&queue, 2);

    state(&view).invoke_play(uid.to_string().into());
    state(&view).invoke_remove(uid.to_string().into());
    state(&view).invoke_play("not-a-uid".into());
    state(&view).invoke_remove("".into());

    assert!(matches!(view.next_command(), Some(Command::PlayUid(played)) if played == uid));
    assert!(matches!(view.next_command(), Some(Command::Remove(removed)) if removed == [uid]));
    assert!(view.next_command().is_none());
}

#[test]
fn given_a_shown_queue_when_a_row_is_dragged_then_it_moves_before_the_target() {
    let queue = queue_of(4);
    let view = open_with_queue(&queue);

    state(&view).invoke_move(3, 1);
    state(&view).invoke_move(-1, 0);
    state(&view).invoke_move(2, 2);

    match view.next_command() {
        Some(Command::Move { uids, before }) => {
            assert_eq!(uids, [uid_at(&queue, 3)]);
            assert_eq!(before, Some(uid_at(&queue, 1)));
        }
        _ => panic!("expected Move"),
    }
    assert!(view.next_command().is_none());
}

#[test]
fn given_a_shown_queue_when_a_new_queue_replaces_it_then_the_revision_changes() {
    let view = open_with_queue(&queue_of(3));
    let revision = state(&view).get_revision();

    set_view(&view.app, queue_of(5).view());

    assert_ne!(state(&view).get_revision(), revision);
    assert_eq!(state(&view).get_rows().row_count(), 5);
}

#[test]
fn given_an_empty_queue_when_it_is_shown_then_nothing_is_current() {
    let view = open_with_queue(&queue_of(2));

    set_view(&view.app, Queue::with_seed(1).view());

    assert_eq!(state(&view).get_current_position(), -1);
    assert!(state(&view).get_current_uid().is_empty());
}
