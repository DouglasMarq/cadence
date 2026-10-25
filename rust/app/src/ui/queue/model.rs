use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use cadence_core::queue::{QueueItem, QueueView, Uid};
use slint::{Model, ModelNotify, ModelTracker};

use crate::ui::{QueueRow, format_duration};

pub type SharedQueueView = Rc<RefCell<QueueView>>;

fn queue_row(item: &QueueItem, is_current: bool) -> QueueRow {
    QueueRow {
        uid: item.uid.to_string().into(),
        title: item.track.title.as_ref().into(),
        artists: item.track.artist_names().into(),
        duration: format_duration(item.track.duration_ms).into(),
        is_current,
    }
}

pub fn row_at(view: &QueueView, position: usize) -> Option<QueueRow> {
    let is_current = view.current_position() == Some(position);
    view.item_at(position)
        .map(|item| queue_row(item, is_current))
}

#[derive(Default)]
pub struct QueueModel {
    view: SharedQueueView,
    notify: ModelNotify,
}

impl QueueModel {
    pub fn shared_view(&self) -> SharedQueueView {
        Rc::clone(&self.view)
    }

    fn uid_at(&self, position: usize) -> Option<Uid> {
        self.view.borrow().item_at(position).map(|item| item.uid)
    }

    pub fn move_target(&self, from: usize, to: usize) -> Option<(Uid, Option<Uid>)> {
        let uid = self.uid_at(from)?;
        if from == to || self.uid_at(to).is_none() {
            return None;
        }
        let before = match to.cmp(&from) {
            Ordering::Less => self.uid_at(to),
            Ordering::Equal | Ordering::Greater => self.uid_at(to + 1),
        };
        Some((uid, before))
    }

    pub fn shares_layout_with(&self, view: &QueueView) -> bool {
        self.view.borrow().shares_layout_with(view)
    }

    pub fn set_view(&self, view: QueueView) {
        let new_position = view.current_position();
        let previous = self.view.replace(view);
        if !previous.shares_layout_with(&self.view.borrow()) {
            self.notify.reset();
            return;
        }
        [previous.current_position(), new_position]
            .into_iter()
            .flatten()
            .for_each(|row| self.notify.row_changed(row));
    }
}

impl Model for QueueModel {
    type Data = QueueRow;

    fn row_count(&self) -> usize {
        self.view.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<QueueRow> {
        row_at(&self.view.borrow(), row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
