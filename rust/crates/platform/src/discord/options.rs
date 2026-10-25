#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiscordDisplayType {
    #[default]
    Cadence,
    ArtistName,
    SongName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordOptions {
    pub display_type: DiscordDisplayType,
    pub is_shown_as_listening: bool,
    pub is_shown_when_paused: bool,
    pub has_state_icon: bool,
}

impl Default for DiscordOptions {
    fn default() -> Self {
        Self {
            display_type: DiscordDisplayType::Cadence,
            is_shown_as_listening: true,
            is_shown_when_paused: true,
            has_state_icon: true,
        }
    }
}
