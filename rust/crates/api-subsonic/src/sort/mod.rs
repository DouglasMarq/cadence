mod albums;
mod artists;
mod key;
mod songs;

use rand::seq::SliceRandom;

pub use albums::sort_albums;
pub use artists::sort_artists;
pub use songs::sort_songs;

pub fn shuffle<T>(items: &mut [T]) {
    items.shuffle(&mut rand::rng());
}
