use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, Timer, TimerMode};

use super::cover_loading::{CoverSlot, load_cover};
use super::results::Results;
use super::searching::{SearchRequest, search, show_more};
use super::song_commands::install_song_commands;
use crate::ui::{AppWindow, Context, SearchState, normalized_search};

const DEBOUNCE: Duration = Duration::from_millis(250);

pub fn install(window: &AppWindow, context: &Context) {
    let state = window.global::<SearchState>();
    let results = Rc::new(Results::default());
    state.set_albums(ModelRc::from(Rc::clone(&results.albums)));
    state.set_artists(ModelRc::from(Rc::clone(&results.artists)));
    state.set_songs(ModelRc::from(results));

    let debounce = Rc::new(Timer::default());
    install_edited(window, context, Rc::clone(&debounce));
    install_submit(window, context, Rc::clone(&debounce));
    install_load(window, context, debounce);
    install_show_more(window, context);
    install_need_cover(window, context);
    install_song_commands(window, context);
}

fn search_now(weak: &slint::Weak<AppWindow>, context: &Context, request: SearchRequest) {
    if let Some(window) = weak.upgrade() {
        search(&window, context, request);
    }
}

fn install_edited(window: &AppWindow, context: &Context, debounce: Rc<Timer>) {
    let weak = window.as_weak();
    let edit_context = context.clone();
    window.global::<SearchState>().on_edited(move |text| {
        let Some(query) = normalized_search(&text) else {
            debounce.stop();
            search_now(
                &weak,
                &edit_context,
                SearchRequest {
                    query: None,
                    is_forced: false,
                },
            );
            return;
        };
        let (weak, context) = (weak.clone(), edit_context.clone());
        debounce.start(TimerMode::SingleShot, DEBOUNCE, move || {
            search_now(
                &weak,
                &context,
                SearchRequest {
                    query: Some(query.clone()),
                    is_forced: false,
                },
            );
        });
    });
}

fn install_submit(window: &AppWindow, context: &Context, debounce: Rc<Timer>) {
    let weak = window.as_weak();
    let submit_context = context.clone();
    window.global::<SearchState>().on_submit(move |text| {
        debounce.stop();
        search_now(
            &weak,
            &submit_context,
            SearchRequest {
                query: normalized_search(&text),
                is_forced: true,
            },
        );
    });
}

fn install_load(window: &AppWindow, context: &Context, debounce: Rc<Timer>) {
    let weak = window.as_weak();
    let load_context = context.clone();
    window.global::<SearchState>().on_load(move |query| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<SearchState>();
        if !query.is_empty() {
            state.set_query(query);
        }
        debounce.stop();
        search(
            &window,
            &load_context,
            SearchRequest {
                query: normalized_search(&state.get_query()),
                is_forced: false,
            },
        );
    });
}

fn install_show_more(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let more_context = context.clone();
    window.global::<SearchState>().on_show_more(move |kind| {
        if let Some(window) = weak.upgrade() {
            show_more(&window, &more_context, kind);
        }
    });
}

fn install_need_cover(window: &AppWindow, context: &Context) {
    let weak = window.as_weak();
    let cover_context = context.clone();
    window
        .global::<SearchState>()
        .on_need_cover(move |kind, index| {
            if let Some(window) = weak.upgrade() {
                load_cover(&window, &cover_context, CoverSlot { kind, index });
            }
        });
}
