use std::sync::{Arc, Mutex, PoisonError};

use cadence_api::{ApiError, MusicServer};
use cadence_core::model::{ArtistDetail, ArtistPart};
use slint::{ComponentHandle, Weak};

use super::super::artists::artist_seed;
use super::super::{AppWindow, ArtistDetailState, Context, on_ui};
use super::current::{begin, is_current, update_current};
use super::view::{clear, render, show_detail, show_loading, show_part};
use crate::views::{View, Views};

const DETAIL_ITEM: &str = "detail";

fn show_failure(window: &AppWindow, error: &ApiError) {
    update_current(|current| {
        current.id.clear();
        current.detail = None;
    });
    show_loading(window, false);
    window
        .global::<ArtistDetailState>()
        .set_error(tr!("Could not load the artist: {}", error).into());
}

#[derive(Clone)]
struct Delivery {
    weak_window: Weak<AppWindow>,
    context: Context,
    generation: u64,
}

impl Delivery {
    fn on_current(&self, update: impl FnOnce(&AppWindow, &Context) + Send + 'static) {
        let Self {
            weak_window,
            context,
            generation,
        } = self.clone();
        on_ui(&weak_window, move |window| {
            if is_current(generation) {
                update(window, &context);
            }
        });
    }

    fn part(&self, part: ArtistPart) {
        self.on_current(move |window, context| show_part(window, context, part));
    }

    fn result(&self, result: Result<ArtistDetail, ApiError>) {
        self.on_current(move |window, context| match result {
            Ok(detail) => show_detail(window, context, detail),
            Err(error) => show_failure(window, &error),
        });
    }
}

struct PartsRequest {
    server: Arc<dyn MusicServer>,
    views: Views,
    artist_id: String,
    delivery: Delivery,
}

impl PartsRequest {
    async fn run(self) -> Result<ArtistDetail, ApiError> {
        let taken_at_ms = Views::taken_now();
        let detail = Mutex::new(None);
        let deliver = |mut part: ArtistPart| {
            self.views.overlay(taken_at_ms, &mut part);
            part.clone()
                .merge_into(&mut detail.lock().unwrap_or_else(PoisonError::into_inner));
            self.delivery.part(part);
        };
        self.server
            .artist_in_parts(&self.artist_id, &deliver)
            .await?;
        detail
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .ok_or(ApiError::NotFound)
    }
}

fn show_seed(window: &AppWindow, context: &Context, artist_id: &str) {
    let Some(profile) = artist_seed(window, artist_id) else {
        return;
    };
    update_current(|current| ArtistPart::Profile(profile).merge_into(&mut current.detail));
    render(window, context);
}

pub fn load(window: &AppWindow, context: &Context, artist_id: &str) {
    let Some(generation) = begin(artist_id) else {
        return;
    };
    context.session.open_view();
    clear(window);
    show_loading(window, true);
    show_seed(window, context, artist_id);
    let Some(server) = context.session.get() else {
        return;
    };
    let views = context.views(&server);
    let delivery = Delivery {
        weak_window: window.as_weak(),
        context: context.clone(),
        generation,
    };
    let request = PartsRequest {
        server,
        views: views.clone(),
        artist_id: artist_id.to_owned(),
        delivery: delivery.clone(),
    };
    let slot = View::Artist(artist_id.to_owned()).slot(DETAIL_ITEM);
    views.load(Some(slot), request.run(), move |result| {
        delivery.result(result);
    });
}
