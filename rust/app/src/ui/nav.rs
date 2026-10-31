use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use super::{AppState, AppWindow, Route};

type Location = (Route, SharedString);
type Step = fn(&mut History, Location) -> Option<Location>;

#[derive(Default)]
struct History {
    back: Vec<Location>,
    forward: Vec<Location>,
}

impl History {
    fn leave(&mut self, current: Location) {
        self.back.push(current);
        self.forward.clear();
    }

    fn step_back(&mut self, current: Location) -> Option<Location> {
        let target = self.back.pop()?;
        self.forward.push(current);
        Some(target)
    }

    fn step_forward(&mut self, current: Location) -> Option<Location> {
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

fn show_location(state: &AppState, (route, detail_id): Location) {
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
            let state = window.global::<AppState>();
            let current = current_location(&state);
            let target = (route, detail_id);
            if current == target {
                return;
            }
            history.borrow_mut().leave(current);
            show_location(&state, target);
            show_reachability(&state, &history.borrow());
        });
}

fn stepping(window: &AppWindow, history: SharedHistory, step: Step) -> impl FnMut() + 'static {
    let weak = window.as_weak();
    move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let state = window.global::<AppState>();
        let Some(target) = step(&mut history.borrow_mut(), current_location(&state)) else {
            return;
        };
        show_location(&state, target);
        show_reachability(&state, &history.borrow());
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
            show_location(&state, (Route::Tracks, SharedString::default()));
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
