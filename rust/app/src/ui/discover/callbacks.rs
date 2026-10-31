use cadence_youtube::music::CHART_COUNTRIES;
use slint::ComponentHandle;

use super::card_actions::{open_card_in_browser, place_card, play_from_card};
use super::cards::CardPosition;
use super::covers::{card_visibility_changed, load_card_cover, load_song_cover, song_rows_shown};
use super::deps::{DiscoverDeps, DiscoverView};
use super::loading::{refresh_sections, show_sections};
use super::model::with_discover;
use super::page::search_detail_id;
use super::page_actions::{
    open_page_in_browser, open_song_in_browser, place_page_song, play_page_from, queue_page,
};
use super::page_loading::open_page;
use super::playing::Placement;
use super::sections::Section;
use crate::ui::{AppState, AppWindow, DiscoverState, Route};

type WindowAction = fn(DiscoverView);
type IndexAction = fn(DiscoverView, usize);
type CardAction = fn(DiscoverView, CardPosition);

fn on_window(view: DiscoverView, action: WindowAction) -> impl Fn() + 'static {
    let (weak_window, deps) = (view.window.as_weak(), view.deps.clone());
    move || {
        if let Some(window) = weak_window.upgrade() {
            action(DiscoverView {
                window: &window,
                deps: &deps,
            });
        }
    }
}

fn on_index(view: DiscoverView, action: IndexAction) -> impl Fn(i32) + 'static {
    let (weak_window, deps) = (view.window.as_weak(), view.deps.clone());
    move |index| {
        if let (Some(window), Ok(index)) = (weak_window.upgrade(), usize::try_from(index)) {
            let view = DiscoverView {
                window: &window,
                deps: &deps,
            };
            action(view, index);
        }
    }
}

fn card_position(shelf: i32, card: i32) -> Option<CardPosition> {
    Some(CardPosition {
        shelf: usize::try_from(shelf).ok()?,
        card: usize::try_from(card).ok()?,
    })
}

fn on_card(view: DiscoverView, action: CardAction) -> impl Fn(i32, i32) + 'static {
    let (weak_window, deps) = (view.window.as_weak(), view.deps.clone());
    move |shelf, card| {
        if let (Some(window), Some(position)) = (weak_window.upgrade(), card_position(shelf, card))
        {
            let view = DiscoverView {
                window: &window,
                deps: &deps,
            };
            action(view, position);
        }
    }
}

fn select_country(view: DiscoverView, index: usize) {
    let Some(country) = CHART_COUNTRIES.get(index) else {
        return;
    };
    with_discover(view.window, |model| {
        model.sections.borrow_mut().chart_country = Some(country.code.to_owned());
    });
    show_sections(view);
}

fn select_mood(view: DiscoverView, index: usize) {
    with_discover(view.window, |model| {
        let mut sections = model.sections.borrow_mut();
        if index < sections.moods().len() {
            sections.mood = Some(index);
        }
    });
    show_sections(view);
}

fn install_open(view: DiscoverView) {
    let DiscoverView { window, deps } = view;
    let (weak_window, open_deps) = (window.as_weak(), deps.clone());
    window.global::<DiscoverState>().on_open(move |detail_id| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        open_deps.context.session.open_view();
        let view = DiscoverView {
            window: &window,
            deps: &open_deps,
        };
        match detail_id.is_empty() {
            true => show_sections(view),
            false => open_page(view, &detail_id),
        }
    });
}

fn install_navigation(view: DiscoverView) {
    install_open(view);
    let DiscoverView { window, deps } = view;
    let state = window.global::<DiscoverState>();
    let (weak_window, section_deps) = (window.as_weak(), deps.clone());
    state.on_select_section(move |section| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        with_discover(&window, |model| {
            model.sections.borrow_mut().section = Section::from_slint(section);
        });
        show_sections(DiscoverView {
            window: &window,
            deps: &section_deps,
        });
    });
    let weak_window = window.as_weak();
    state.on_search(move |query| {
        if let (Some(window), Some(detail_id)) = (weak_window.upgrade(), search_detail_id(&query)) {
            window
                .global::<AppState>()
                .invoke_navigate(Route::Discover, detail_id.into());
        }
    });
    state.on_refresh(on_window(view, refresh_sections));
    state.on_select_country(on_index(view, select_country));
    state.on_select_mood(on_index(view, select_mood));
}

fn install_visibility(window: &AppWindow) {
    let state = window.global::<DiscoverState>();
    let weak_window = window.as_weak();
    state.on_card_visibility_changed(move |shelf, card, is_visible| {
        if let (Some(window), Some(position)) = (weak_window.upgrade(), card_position(shelf, card))
        {
            card_visibility_changed(&window, position, is_visible);
        }
    });
    let weak_window = window.as_weak();
    state.on_song_rows_shown(move |first_row, row_count| {
        if let (Some(window), Ok(first_row), Ok(row_count)) = (
            weak_window.upgrade(),
            usize::try_from(first_row),
            usize::try_from(row_count),
        ) {
            song_rows_shown(&window, first_row, row_count);
        }
    });
}

fn install_cards(view: DiscoverView) {
    let state = view.window.global::<DiscoverState>();
    state.on_cover_needed(on_card(view, load_card_cover));
    state.on_play_from_card(on_card(view, play_from_card));
    state.on_play_card(on_card(view, |view, position| {
        place_card(view, position, Placement::PlayFrom(0));
    }));
    state.on_play_card_next(on_card(view, |view, position| {
        place_card(view, position, Placement::Next);
    }));
    state.on_queue_card(on_card(view, |view, position| {
        place_card(view, position, Placement::Last);
    }));
    state.on_open_card_in_browser(on_card(view, open_card_in_browser));
}

fn install_page(view: DiscoverView) {
    let state = view.window.global::<DiscoverState>();
    state.on_song_cover_needed(on_index(view, load_song_cover));
    state.on_play_song(on_index(view, play_page_from));
    state.on_play_song_next(on_index(view, |view, index| {
        place_page_song(view, index, Placement::Next);
    }));
    state.on_queue_song(on_index(view, |view, index| {
        place_page_song(view, index, Placement::Last);
    }));
    state.on_open_song_in_browser(on_index(view, open_song_in_browser));
    state.on_play_page(on_window(view, |view| play_page_from(view, 0)));
    state.on_queue_page(on_window(view, queue_page));
    state.on_open_page_in_browser(on_window(view, open_page_in_browser));
}

pub fn install_callbacks(window: &AppWindow, deps: &DiscoverDeps) {
    let view = DiscoverView { window, deps };
    install_navigation(view);
    install_cards(view);
    install_visibility(window);
    install_page(view);
}
