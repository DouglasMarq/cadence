use std::collections::HashSet;
use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::Playlist;
use slint::ComponentHandle;

use super::streamed::{ListedAdd, ListedWrite, add_listed_songs, add_listed_to_existing};
use crate::ui::tracks::ListedSongs;
use crate::ui::{AppWindow, Context, on_ui, playlist_detail, playlists, show_status};
use crate::views::View;

pub struct PickedPlaylist {
    pub id: String,
    pub name: String,
}

pub struct PlaylistWrite<Target, Songs> {
    pub server: Arc<dyn MusicServer>,
    pub target: Target,
    pub songs: Songs,
}

fn progress_reporter(
    window: &AppWindow,
    name: &str,
    total: usize,
) -> impl Fn(usize) + Send + 'static {
    let (weak, name) = (window.as_weak(), name.to_owned());
    move |read| {
        let message = tr_plural!(
            "Adding to \"{}\": {} of {n} song",
            "Adding to \"{}\": {} of {n} songs",
            total,
            name,
            read
        );
        on_ui(&weak, move |window| show_status(window, &message));
    }
}

pub fn add_listed(
    window: &AppWindow,
    context: &Context,
    write: PlaylistWrite<PickedPlaylist, ListedSongs>,
) {
    let PlaylistWrite {
        server,
        target: picked,
        songs: listed,
    } = write;
    let report = progress_reporter(window, &picked.name, listed.song_count());
    report(0);
    let weak = window.as_weak();
    let work = context.session.background_work();
    let views = context.views(&server);
    context.runtime.spawn(async move {
        let listed_write = ListedWrite {
            server: server.as_ref(),
            playlist_id: &picked.id,
            report,
        };
        let adding = add_listed_to_existing(&listed_write, listed);
        if let Some(outcome) = work.run(adding).await {
            views.invalidate_playlist(&picked.id).await;
            on_ui(&weak, move |window| finish_add(window, &picked, outcome));
        }
    });
}

fn finish_add(window: &AppWindow, picked: &PickedPlaylist, outcome: ListedAdd) {
    let (name, added) = (&picked.name, outcome.added);
    let message = match (outcome.failure, added) {
        (None, 0) => tr!("Already in \"{}\"", name),
        (None, _) => tr_plural!(
            "Added {n} song to \"{}\"",
            "Added {n} songs to \"{}\"",
            added,
            name
        ),
        (Some(error), 0) => tr!("Could not add to \"{}\": {}", name, error),
        (Some(error), _) => tr_plural!(
            "Added only {n} song to \"{}\": {}",
            "Added only {n} songs to \"{}\": {}",
            added,
            name,
            error
        ),
    };
    show_status(window, &message);
    if outcome.added > 0 {
        playlists::refresh(window);
        playlist_detail::refresh_if_showing(window, &picked.id);
    }
}

pub fn create_listed(
    window: &AppWindow,
    context: &Context,
    write: PlaylistWrite<String, ListedSongs>,
) {
    let report = progress_reporter(window, &write.target, write.songs.song_count());
    report(0);
    let weak = window.as_weak();
    let work = context.session.background_work();
    let views = context.views(&write.server);
    context.runtime.spawn(async move {
        let creating = create_and_fill(write, report);
        let Some((created, outcome)) = work.run(creating).await else {
            return;
        };
        views.invalidate(View::Playlists).await;
        on_ui(&weak, move |window| {
            finish_create(window, created.map(|playlist| playlist.name), outcome);
        });
    });
}

async fn create_and_fill(
    write: PlaylistWrite<String, ListedSongs>,
    report: impl Fn(usize),
) -> (cadence_api::Result<Playlist>, ListedAdd) {
    let created = write.server.create_playlist(&write.target, &[]).await;
    let outcome = match &created {
        Ok(playlist) => {
            let listed_write = ListedWrite {
                server: write.server.as_ref(),
                playlist_id: &playlist.id,
                report,
            };
            add_listed_songs(&listed_write, write.songs, HashSet::new()).await
        }
        Err(_) => ListedAdd::default(),
    };
    (created, outcome)
}

fn finish_create(window: &AppWindow, created: cadence_api::Result<String>, outcome: ListedAdd) {
    let added = outcome.added;
    let message = match (created, outcome.failure) {
        (Err(error), _) => tr!("Could not create the playlist: {}", error),
        (Ok(name), None) => tr_plural!(
            "Created \"{}\" with {n} song",
            "Created \"{}\" with {n} songs",
            added,
            name
        ),
        (Ok(name), Some(error)) => tr_plural!(
            "Created \"{}\" with only {n} song: {}",
            "Created \"{}\" with only {n} songs: {}",
            added,
            name,
            error
        ),
    };
    show_status(window, &message);
    playlists::refresh(window);
}
