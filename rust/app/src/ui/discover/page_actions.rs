use super::browser::open_in_browser;
use super::deps::DiscoverView;
use super::model::with_discover;
use super::playing::{Placement, QueueRequest, play_songs, send_resolved};
use crate::ui::AppWindow;

fn page_songs(window: &AppWindow) -> Vec<cadence_youtube::music::MusicSong> {
    with_discover(window, |model| model.page.borrow().songs.clone()).unwrap_or_default()
}

pub fn play_page_from(view: DiscoverView, index: usize) {
    let songs = page_songs(view.window);
    if index < songs.len() {
        play_songs(view, songs, Placement::PlayFrom(index));
    }
}

pub fn place_page_song(view: DiscoverView, index: usize, placement: Placement) {
    if let Some(song) = with_discover(view.window, |model| model.page_song(index)).flatten() {
        play_songs(view, vec![song], placement);
    }
}

pub fn queue_page(view: DiscoverView) {
    let songs = page_songs(view.window);
    let title =
        with_discover(view.window, |model| model.page.borrow().title.clone()).unwrap_or_default();
    let request = QueueRequest {
        songs,
        placement: Placement::Last,
        collection: Some(title),
    };
    send_resolved(view, request);
}

pub fn open_song_in_browser(view: DiscoverView, index: usize) {
    if let Some(song) = with_discover(view.window, |model| model.page_song(index)).flatten() {
        open_in_browser(view, song.web_url());
    }
}

pub fn open_page_in_browser(view: DiscoverView) {
    let url = with_discover(view.window, |model| {
        model
            .page
            .borrow()
            .target
            .as_ref()
            .and_then(|target| target.web_url())
    })
    .flatten();
    if let Some(url) = url {
        open_in_browser(view, url);
    }
}
