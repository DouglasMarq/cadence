use cadence_youtube::music::ShelfItem;

use super::browser::open_in_browser;
use super::cards::CardPosition;
use super::deps::DiscoverView;
use super::model::with_discover;
use super::playing::{Placement, play_collection, play_songs};
use crate::discover::mixes::is_mix_id;
use crate::ui::AppWindow;

fn item_at(window: &AppWindow, position: CardPosition) -> Option<ShelfItem> {
    with_discover(window, |model| model.item(position.shelf, position.card)).flatten()
}

pub fn play_from_card(view: DiscoverView, position: CardPosition) {
    let shelf_songs = with_discover(view.window, |model| {
        model.shelf_songs_from(position.shelf, position.card)
    })
    .flatten();
    if let Some((songs, start)) = shelf_songs {
        play_songs(view, songs, Placement::PlayFrom(start));
    }
}

pub fn place_card(view: DiscoverView, position: CardPosition, placement: Placement) {
    match item_at(view.window, position) {
        Some(ShelfItem::Song(song)) => play_songs(view, vec![song], placement),
        Some(ShelfItem::Collection(collection)) => {
            play_collection(view, collection.browse_id, placement);
        }
        None => {}
    }
}

pub fn open_card_in_browser(view: DiscoverView, position: CardPosition) {
    let url = match item_at(view.window, position) {
        Some(ShelfItem::Song(song)) => song.web_url(),
        Some(ShelfItem::Collection(collection)) if !is_mix_id(&collection.browse_id) => {
            collection.web_url()
        }
        Some(ShelfItem::Collection(_)) | None => return,
    };
    open_in_browser(view, url);
}
