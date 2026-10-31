use std::cmp::Ordering;

use slint::{Model, ModelNotify, ModelRc, VecModel};

pub fn sync_rows<Row: Clone + PartialEq + 'static>(model: &VecModel<Row>, rows: Vec<Row>) {
    for surplus in (rows.len()..model.row_count()).rev() {
        model.remove(surplus);
    }
    for (index, row) in rows.into_iter().enumerate() {
        match model.row_data(index) {
            Some(shown) if shown == row => {}
            Some(_) => model.set_row_data(index, row),
            None => model.push(row),
        }
    }
}

pub fn show_rows<Row: Clone + PartialEq + 'static>(
    shown: &ModelRc<Row>,
    rows: Vec<Row>,
    install: impl FnOnce(ModelRc<Row>),
) {
    match shown.as_any().downcast_ref::<VecModel<Row>>() {
        Some(model) => sync_rows(model, rows),
        None => install(ModelRc::new(VecModel::from(rows))),
    }
}

pub fn notify_row_count(notify: &ModelNotify, before: usize, after: usize) {
    match after.cmp(&before) {
        Ordering::Equal => {}
        Ordering::Greater if before == 0 => notify.reset(),
        Ordering::Greater => notify.row_added(before, after - before),
        Ordering::Less => notify.row_removed(after, before - after),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(model: &VecModel<u32>) -> Vec<u32> {
        model.iter().collect()
    }

    #[test]
    fn given_the_same_rows_when_synced_then_nothing_changes() {
        let model = VecModel::from(vec![1, 2, 3]);
        sync_rows(&model, vec![1, 2, 3]);
        assert_eq!(rows(&model), [1, 2, 3]);
    }

    #[test]
    fn given_fewer_rows_when_synced_then_the_tail_goes() {
        let model = VecModel::from(vec![1, 2, 3]);
        sync_rows(&model, vec![1]);
        assert_eq!(rows(&model), [1]);
    }

    #[test]
    fn given_more_rows_when_synced_then_they_are_appended() {
        let model = VecModel::from(vec![1]);
        sync_rows(&model, vec![1, 2, 3]);
        assert_eq!(rows(&model), [1, 2, 3]);
    }

    #[test]
    fn given_changed_rows_when_synced_then_they_are_replaced() {
        let model = VecModel::from(vec![1, 2, 3]);
        sync_rows(&model, vec![1, 9, 3]);
        assert_eq!(rows(&model), [1, 9, 3]);
    }

    #[test]
    fn given_a_vec_model_when_shown_then_it_is_kept_and_updated_in_place() {
        let model = ModelRc::new(VecModel::from(vec![1_u32, 2]));
        let mut replaced = None;

        show_rows(&model, vec![1, 3], |installed| replaced = Some(installed));

        assert!(replaced.is_none());
        assert_eq!(model.row_data(1), Some(3));
    }

    #[test]
    fn given_no_vec_model_when_shown_then_a_new_one_is_installed() {
        let empty: ModelRc<u32> = ModelRc::default();
        let mut replaced = None;

        show_rows(&empty, vec![7], |installed| replaced = Some(installed));

        assert_eq!(replaced.and_then(|model| model.row_data(0)), Some(7));
    }

    #[test]
    fn given_any_count_change_when_notified_then_an_unwatched_model_is_unaffected() {
        let notify = ModelNotify::default();
        notify_row_count(&notify, 0, 0);
        notify_row_count(&notify, 0, 3);
        notify_row_count(&notify, 3, 5);
        notify_row_count(&notify, 5, 2);
    }
}
