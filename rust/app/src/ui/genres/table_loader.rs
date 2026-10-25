use slint::{ComponentHandle, ModelRc};

use super::binding::{Binding, with_table};
use super::track_table::TrackTable;
use crate::ui::tracks::{Fetched, FetchedCount, FetchedPage, spawn_fetch};
use crate::ui::{AppWindow, Context, TrackItem, on_ui, show_status};

impl Binding<TrackItem> {
    pub fn track_table(
        self,
        window: &AppWindow,
        context: &Context,
        is_favorite: bool,
    ) -> ModelRc<TrackItem> {
        let weak = window.as_weak();
        let context = context.clone();
        ModelRc::new(TrackTable::new(is_favorite, move |request| {
            let window = weak.clone();
            spawn_fetch(&context, request, move |fetched| {
                on_ui(&window, move |window| match fetched {
                    Fetched::Count(count) => apply_track_count(window, self, count),
                    Fetched::Page(page) => apply_track_page(window, self, page),
                });
            });
        }))
    }
}

fn apply_track_count(window: &AppWindow, binding: Binding<TrackItem>, count: FetchedCount) {
    let total = count.result.as_ref().map_or(0, |total| *total);
    let is_current = with_table(window, binding, |table| table.is_current(count.generation));
    if is_current != Some(true) {
        return;
    }
    with_table(window, binding, |table| {
        table.apply_count(count.generation, total);
    });
    (binding.loading)(window, false);
    if let Err(error) = count.result {
        show_status(window, &tr!("Could not load tracks: {}", error));
    }
}

fn apply_track_page(window: &AppWindow, binding: Binding<TrackItem>, fetched: FetchedPage) {
    let FetchedPage {
        generation,
        page,
        result,
    } = fetched;
    match result {
        Ok(songs) => {
            with_table(window, binding, |table| {
                table.apply_listed_page(generation, page, songs);
            });
        }
        Err(error) => {
            with_table(window, binding, |table| table.page_failed(generation, page));
            show_status(window, &tr!("Could not load tracks: {}", error));
        }
    }
}
