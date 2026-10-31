use cadence_storage::view_cache::ViewKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Home,
    Tracks,
    Albums,
    Artists,
    Favorites,
    Genres,
    Playlists,
    Folders,
    Radio,
    Discover,
    Album(String),
    Artist(String),
    Playlist(String),
    Edits,
}

impl View {
    fn group(&self) -> String {
        match self {
            Self::Home => "home".to_owned(),
            Self::Tracks => "tracks".to_owned(),
            Self::Albums => "albums".to_owned(),
            Self::Artists => "artists".to_owned(),
            Self::Favorites => "favorites".to_owned(),
            Self::Genres => "genres".to_owned(),
            Self::Playlists => "playlists".to_owned(),
            Self::Folders => "folders".to_owned(),
            Self::Radio => "radio".to_owned(),
            Self::Discover => "discover".to_owned(),
            Self::Album(id) => format!("album/{id}"),
            Self::Artist(id) => format!("artist/{id}"),
            Self::Playlist(id) => format!("playlist/{id}"),
            Self::Edits => "edits".to_owned(),
        }
    }

    pub fn slot(self, item: impl Into<String>) -> ViewSlot {
        ViewSlot {
            view: self,
            item: item.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSlot {
    view: View,
    item: String,
}

impl ViewSlot {
    pub(super) fn key(&self, server_id: &str) -> ViewKey {
        ViewKey::new(server_id, &self.view.group(), &self.item)
    }
}
