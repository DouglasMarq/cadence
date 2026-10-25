use std::sync::Arc;

use slint::ComponentHandle;

use super::covers::load_page_cover;
use super::deps::DiscoverView;
use super::mixes::mix_page;
use super::model::with_discover;
use super::page::PageTarget;
use super::render::render_page;
use crate::discover::mixes::MixId;
use crate::ui::{AppWindow, DiscoverState, on_ui, show_status};

pub fn open_page(view: DiscoverView, detail_id: &str) {
    let window = view.window;
    let Some(target) = PageTarget::parse(detail_id) else {
        show_status(window, &tr!("This Discover page does not exist"));
        return;
    };
    let generation = with_discover(window, |model| {
        let generation = model.page.borrow_mut().open(target.clone());
        if generation.is_some() {
            model.show_page_songs();
        }
        generation
    })
    .flatten();
    if let Some(generation) = generation {
        window.global::<DiscoverState>().set_has_page_cover(false);
        match target {
            PageTarget::Mix(mix) => show_mix_page(view, mix, generation),
            other => fetch_page(view, other, generation),
        }
    }
    render_page(window);
}

fn show_mix_page(view: DiscoverView, mix: MixId, generation: u64) {
    let window = view.window;
    let result = mix_page(window, mix).ok_or_else(|| tr!("This mix is no longer available"));
    let is_current = is_page_current(window, generation);
    with_discover(window, |model| {
        model.page.borrow_mut().show_collection(generation, result);
    });
    page_loaded(view, generation, is_current);
}

fn is_page_current(window: &AppWindow, generation: u64) -> bool {
    with_discover(window, |model| model.page.borrow().is_current(generation)) == Some(true)
}

fn fetch_page(DiscoverView { window, deps }: DiscoverView, target: PageTarget, generation: u64) {
    let source = Arc::clone(&deps.source);
    let settings = deps.settings();
    let weak_window = window.as_weak();
    let cover_deps = deps.clone();
    deps.context.runtime.spawn(async move {
        match target {
            PageTarget::Collection { browse_id, .. } => {
                let result = source.collection(browse_id, settings).await;
                let result = result.map_err(|error| error.to_string());
                on_ui(&weak_window, move |window| {
                    let is_current = is_page_current(window, generation);
                    with_discover(window, |model| {
                        model.page.borrow_mut().show_collection(generation, result);
                    });
                    let view = DiscoverView {
                        window,
                        deps: &cover_deps,
                    };
                    page_loaded(view, generation, is_current);
                });
            }
            PageTarget::Mix(_) => {}
            PageTarget::Search(query) => {
                let result = source.search(query, settings).await;
                let result = result.map_err(|error| error.to_string());
                on_ui(&weak_window, move |window| {
                    let is_current = is_page_current(window, generation);
                    with_discover(window, |model| {
                        model.page.borrow_mut().show_search(generation, result);
                    });
                    let view = DiscoverView {
                        window,
                        deps: &cover_deps,
                    };
                    page_loaded(view, generation, is_current);
                });
            }
        }
    });
}

fn page_loaded(view: DiscoverView, generation: u64, is_applied: bool) {
    if !is_applied {
        return;
    }
    let window = view.window;
    let loaded = with_discover(window, |model| {
        model.show_page_songs();
        let page = model.page.borrow();
        (page.error.clone(), page.thumbnail_url.clone())
    });
    let Some((error, thumbnail_url)) = loaded else {
        return;
    };
    if let Some(error) = error {
        show_status(window, &tr!("Could not load this page: {}", error));
    }
    if let Some(url) = thumbnail_url {
        load_page_cover(view, generation, url);
    }
    render_page(window);
}
