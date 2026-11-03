use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use super::{AppState, AppWindow, PageScroll, Route};

type Location = (Route, SharedString);
type Step = fn(&mut History, Visit) -> Option<Visit>;

const PAGE_TOP: f32 = 0.0;

struct Visit {
    location: Location,
    scroll_y: f32,
}

impl Visit {
    fn at_top(location: Location) -> Self {
        Self {
            location,
            scroll_y: PAGE_TOP,
        }
    }
}

#[derive(Default)]
struct History {
    back: Vec<Visit>,
    forward: Vec<Visit>,
}

impl History {
    fn leave(&mut self, current: Visit) {
        self.back.push(current);
        self.forward.clear();
    }

    fn step_back(&mut self, current: Visit) -> Option<Visit> {
        let target = self.back.pop()?;
        self.forward.push(current);
        Some(target)
    }

    fn step_forward(&mut self, current: Visit) -> Option<Visit> {
        let target = self.forward.pop()?;
        self.back.push(current);
        Some(target)
    }

    fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
    }
}

type SharedHistory = Rc<RefCell<History>>;

fn current_location(state: &AppState) -> Location {
    (state.get_route(), state.get_detail_id())
}

fn current_visit(window: &AppWindow) -> Visit {
    Visit {
        location: current_location(&window.global::<AppState>()),
        scroll_y: window.global::<PageScroll>().get_shown_y(),
    }
}

fn request_scroll(window: &AppWindow, scroll_y: f32) {
    let scroll = window.global::<PageScroll>();
    scroll.set_restore_y(scroll_y);
    scroll.set_restore_request(scroll.get_restore_request().wrapping_add(1));
}

fn show_visit(window: &AppWindow, visit: Visit) {
    request_scroll(window, visit.scroll_y);
    let (route, detail_id) = visit.location;
    let state = window.global::<AppState>();
    state.set_detail_id(detail_id);
    state.set_route(route);
}

fn show_reachability(state: &AppState, history: &History) {
    state.set_can_go_back(!history.back.is_empty());
    state.set_can_go_forward(!history.forward.is_empty());
}

fn is_session_bound((route, detail_id): &Location) -> bool {
    !detail_id.is_empty() || *route == Route::NowPlaying
}

fn install_navigate(window: &AppWindow, history: SharedHistory) {
    let weak = window.as_weak();
    window
        .global::<AppState>()
        .on_navigate(move |route, detail_id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let current = current_visit(&window);
            let target = (route, detail_id);
            if current.location == target {
                return;
            }
            history.borrow_mut().leave(current);
            show_visit(&window, Visit::at_top(target));
            show_reachability(&window.global::<AppState>(), &history.borrow());
        });
}

fn stepping(window: &AppWindow, history: SharedHistory, step: Step) -> impl FnMut() + 'static {
    let weak = window.as_weak();
    move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let Some(target) = step(&mut history.borrow_mut(), current_visit(&window)) else {
            return;
        };
        show_visit(&window, target);
        show_reachability(&window.global::<AppState>(), &history.borrow());
    }
}

fn install_reset(window: &AppWindow, history: SharedHistory) {
    let weak = window.as_weak();
    window.global::<AppState>().on_reset_history(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<AppState>();
        history.borrow_mut().clear();
        show_reachability(&state, &history.borrow());
        if is_session_bound(&current_location(&state)) {
            show_visit(
                &window,
                Visit::at_top((Route::Tracks, SharedString::default())),
            );
        }
    });
}

pub fn install(window: &AppWindow) {
    let history = SharedHistory::default();
    let state = window.global::<AppState>();
    install_navigate(window, Rc::clone(&history));
    state.on_back(stepping(window, Rc::clone(&history), History::step_back));
    state.on_forward(stepping(window, Rc::clone(&history), History::step_forward));
    install_reset(window, history);
}
