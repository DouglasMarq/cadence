use super::detail::SharedDetail;
use crate::ui::Context;
use crate::ui::playlist_picker::PlaylistPicker;

#[derive(Clone)]
pub struct DetailDeps {
    pub context: Context,
    pub shared: SharedDetail,
    pub picker: PlaylistPicker,
}
