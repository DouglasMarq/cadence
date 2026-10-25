use cadence_core::model::Playlist;

pub(crate) fn with_cache_busted_cover(playlist: Playlist) -> Playlist {
    let cover_art_id = cache_busted_cover_art(
        playlist.cover_art_id.clone(),
        playlist.updated_at.as_deref(),
    );
    Playlist {
        cover_art_id,
        ..playlist
    }
}

fn cache_busted_cover_art(cover_art: Option<String>, changed: Option<&str>) -> Option<String> {
    cover_art.map(|cover_art| match changed {
        Some(changed) if !changed.is_empty() => format!("{cover_art}&_={changed}"),
        _ => cover_art,
    })
}
