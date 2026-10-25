use super::picker::SharedPicker;
use crate::ui::Context;

#[derive(Clone)]
pub struct PlaylistPicker {
    pub(super) shared: SharedPicker,
    pub(super) context: Context,
}

impl PlaylistPicker {
    pub fn new(shared: SharedPicker, context: Context) -> Self {
        Self { shared, context }
    }
}
