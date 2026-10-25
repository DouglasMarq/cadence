use serde_json::Value;

use super::items::{SongDefaults, list_item_song};
use super::nodes::{array, best_thumbnail, text_of};
use crate::music::model::{CollectionKind, CollectionPage};

const HEADER_SECTIONS: &str = "/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0";
const HEADER: &str = "/musicResponsiveHeaderRenderer";
const EDITABLE_HEADER: &str =
    "/musicEditablePlaylistDetailHeaderRenderer/header/musicResponsiveHeaderRenderer";
const TRACK_SECTIONS: &str =
    "/contents/twoColumnBrowseResultsRenderer/secondaryContents/sectionListRenderer/contents";
const HEADER_THUMBNAIL: &str = "/thumbnail/musicThumbnailRenderer/thumbnail";
const DETAIL_SEPARATOR: &str = " • ";

pub(crate) fn collection_page(response: &Value, kind: CollectionKind) -> Option<CollectionPage> {
    let header_node = response.pointer(HEADER_SECTIONS).and_then(|section| {
        section
            .pointer(HEADER)
            .or_else(|| section.pointer(EDITABLE_HEADER))
    });
    let header = collection_header(header_node);
    let defaults = song_defaults(kind, &header);
    let songs: Vec<_> = array(response, TRACK_SECTIONS)
        .iter()
        .flat_map(track_rows)
        .filter_map(|row| row.get("musicResponsiveListItemRenderer"))
        .filter_map(|renderer| list_item_song(renderer, &defaults))
        .collect();
    if header_node.is_none() && songs.is_empty() {
        return None;
    }
    Some(CollectionPage {
        subtitle: joined_details(&[&header.owner, &header.details]),
        title: header.title,
        thumbnail_url: header.thumbnail_url,
        songs,
    })
}

struct CollectionHeader {
    title: String,
    owner: String,
    details: String,
    thumbnail_url: Option<String>,
}

fn collection_header(header: Option<&Value>) -> CollectionHeader {
    CollectionHeader {
        title: text_of(header.and_then(|header| header.get("title"))),
        owner: text_of(header.and_then(|header| header.get("straplineTextOne"))),
        details: text_of(header.and_then(|header| header.get("subtitle"))),
        thumbnail_url: best_thumbnail(header.and_then(|header| header.pointer(HEADER_THUMBNAIL))),
    }
}

fn song_defaults(kind: CollectionKind, header: &CollectionHeader) -> SongDefaults {
    match kind {
        CollectionKind::Album => SongDefaults {
            album: Some(header.title.clone()).filter(|title| !title.is_empty()),
            artists: Some(header.owner.clone())
                .filter(|owner| !owner.is_empty())
                .into_iter()
                .collect(),
            thumbnail_url: header.thumbnail_url.clone(),
        },
        CollectionKind::Playlist => SongDefaults::default(),
    }
}

fn track_rows(section: &Value) -> &[Value] {
    match section.get("musicShelfRenderer") {
        Some(shelf) => array(shelf, "/contents"),
        None => array(section, "/musicPlaylistShelfRenderer/contents"),
    }
}

fn joined_details(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(DETAIL_SEPARATOR)
}
