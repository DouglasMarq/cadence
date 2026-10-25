use cadence_storage::settings::LyricsSourceSetting;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LyricsSource {
    pub(super) source: LyricsSourceSetting,
    pub(super) is_enabled: bool,
}

pub(super) fn lyrics_sources(enabled: &[LyricsSourceSetting]) -> Vec<LyricsSource> {
    let mut sources: Vec<LyricsSource> = Vec::new();
    for &source in enabled {
        push_if_missing(&mut sources, source, true);
    }
    for source in LyricsSourceSetting::ALL {
        push_if_missing(&mut sources, source, false);
    }
    sources
}

fn push_if_missing(sources: &mut Vec<LyricsSource>, source: LyricsSourceSetting, is_enabled: bool) {
    if sources.iter().any(|listed| listed.source == source) {
        return;
    }
    sources.push(LyricsSource { source, is_enabled });
}

pub(super) fn enabled_lyrics_sources(sources: &[LyricsSource]) -> Vec<LyricsSourceSetting> {
    sources
        .iter()
        .filter(|source| source.is_enabled)
        .map(|source| source.source)
        .collect()
}

pub(super) fn lyrics_move_target(
    sources: &[LyricsSource],
    index: usize,
    delta: isize,
) -> Option<usize> {
    let target = index.checked_add_signed(delta)?;
    (index < sources.len() && target < sources.len()).then_some(target)
}

#[cfg(test)]
mod tests {
    use cadence_storage::settings::IntegrationSettings;

    use super::*;

    #[test]
    fn lyrics_sources_list_enabled_first_then_the_rest() {
        let mut sources = lyrics_sources(&[
            LyricsSourceSetting::Lrclib,
            LyricsSourceSetting::Server,
            LyricsSourceSetting::Lrclib,
        ]);
        let listed: Vec<_> = sources
            .iter()
            .map(|source| (source.source, source.is_enabled))
            .collect();
        assert_eq!(
            listed,
            [
                (LyricsSourceSetting::Lrclib, true),
                (LyricsSourceSetting::Server, true),
                (LyricsSourceSetting::NetEase, false),
                (LyricsSourceSetting::Genius, false)
            ]
        );
        assert_eq!(lyrics_move_target(&sources, 1, -1), Some(0));
        sources.swap(1, 0);
        assert_eq!(lyrics_move_target(&sources, 0, -1), None);
        assert_eq!(lyrics_move_target(&sources, 3, 1), None);
        sources[3].is_enabled = true;
        assert_eq!(
            enabled_lyrics_sources(&sources),
            [
                LyricsSourceSetting::Server,
                LyricsSourceSetting::Lrclib,
                LyricsSourceSetting::Genius
            ]
        );
    }

    #[test]
    fn default_lyrics_sources_survive_a_round_trip() {
        let defaults = IntegrationSettings::default().lyrics_sources;
        assert_eq!(enabled_lyrics_sources(&lyrics_sources(&defaults)), defaults);
    }
}
