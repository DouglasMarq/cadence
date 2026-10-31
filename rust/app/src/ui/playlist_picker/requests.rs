use std::sync::Arc;

use cadence_api::MusicServer;
use cadence_core::model::Playlist;
use slint::ComponentHandle;

use super::choices::new_song_ids;
use super::dialog::close;
use super::handle::PlaylistPicker;
use super::listed_requests::{PickedPlaylist, PlaylistWrite, add_listed, create_listed};
use super::picker::{PendingRequest, PickedSongs, SharedPicker, lock};
use super::playlist_writes::exclusive_playlist_write;
use crate::ui::{AppWindow, PlaylistPickerState, on_ui, playlist_detail, playlists, show_status};
use crate::views::View;

enum RequestOutcome {
    Succeeded(String),
    Failed(String),
}

fn start_request(window: &AppWindow, shared: &SharedPicker) -> Option<PendingRequest> {
    let pending = lock(shared).start_request()?;
    window.global::<PlaylistPickerState>().set_is_busy(true);
    Some(pending)
}

struct FinishedRequest {
    generation: u64,
    outcome: RequestOutcome,
}

fn finish_request(window: &AppWindow, shared: &SharedPicker, finished: FinishedRequest) {
    let mut picker = lock(shared);
    if !picker.is_current(finished.generation) {
        return;
    }
    picker.finish_request(finished.generation);
    drop(picker);
    window.global::<PlaylistPickerState>().set_is_busy(false);
    match finished.outcome {
        RequestOutcome::Succeeded(message) => {
            close(window, shared);
            show_status(window, &message);
        }
        RequestOutcome::Failed(message) => show_status(window, &message),
    }
}

pub fn pick(window: &AppWindow, picker: &PlaylistPicker, playlist_id: String) {
    let Some(server) = picker.context.session.get() else {
        return;
    };
    let shared = &picker.shared;
    let Some(PendingRequest { generation, songs }) = start_request(window, shared) else {
        return;
    };
    let picked = PickedPlaylist {
        name: lock(shared).playlist_name(&playlist_id),
        id: playlist_id,
    };
    match songs {
        PickedSongs::Ids(song_ids) => {
            let write = PlaylistWrite {
                server,
                target: picked,
                songs: IdsRequest {
                    generation,
                    song_ids,
                },
            };
            pick_ids(window, picker, write);
        }
        PickedSongs::Listed(listed) => {
            close(window, shared);
            let write = PlaylistWrite {
                server,
                target: picked,
                songs: listed,
            };
            add_listed(window, &picker.context, write);
        }
    }
}

struct IdsRequest {
    generation: u64,
    song_ids: Vec<String>,
}

fn pick_ids(
    window: &AppWindow,
    picker: &PlaylistPicker,
    write: PlaylistWrite<PickedPlaylist, IdsRequest>,
) {
    let (weak, shared) = (window.as_weak(), Arc::clone(&picker.shared));
    let views = picker.context.views(&write.server);
    picker.context.runtime.spawn(async move {
        let PlaylistWrite {
            server,
            target,
            songs: request,
        } = write;
        let added = add_new_songs(server.as_ref(), &target.id, &request.song_ids).await;
        if matches!(added, Ok(count) if count > 0) {
            views.invalidate_playlist(&target.id).await;
        }
        on_ui(&weak, move |window| {
            let finished = FinishedPick {
                generation: request.generation,
                playlist: target,
                added,
            };
            finish_pick(window, &shared, finished);
        });
    });
}

async fn add_new_songs(
    server: &dyn MusicServer,
    playlist_id: &str,
    song_ids: &[String],
) -> cadence_api::Result<usize> {
    let _exclusive = exclusive_playlist_write(playlist_id).await;
    let existing = server.playlist_songs(playlist_id).await?;
    let added_ids = new_song_ids(&existing, song_ids);
    if !added_ids.is_empty() {
        server.add_to_playlist(playlist_id, &added_ids).await?;
    }
    Ok(added_ids.len())
}

struct FinishedPick {
    generation: u64,
    playlist: PickedPlaylist,
    added: cadence_api::Result<usize>,
}

fn finish_pick(window: &AppWindow, shared: &SharedPicker, finished: FinishedPick) {
    let name = &finished.playlist.name;
    let has_changed = matches!(finished.added, Ok(added) if added > 0);
    let outcome = match finished.added {
        Ok(0) => RequestOutcome::Succeeded(tr!("Already in \"{}\"", name)),
        Ok(added) => RequestOutcome::Succeeded(tr_plural!(
            "Added {n} song to \"{}\"",
            "Added {n} songs to \"{}\"",
            added,
            name
        )),
        Err(error) => RequestOutcome::Failed(tr!("Could not add to \"{}\": {}", name, error)),
    };
    let generation = finished.generation;
    finish_request(
        window,
        shared,
        FinishedRequest {
            generation,
            outcome,
        },
    );
    if has_changed {
        playlists::refresh(window);
        playlist_detail::refresh_if_showing(window, &finished.playlist.id);
    }
}

pub fn create(window: &AppWindow, picker: &PlaylistPicker, name: &str) {
    let name = name.trim().to_owned();
    let Some(server) = picker.context.session.get().filter(|_| !name.is_empty()) else {
        return;
    };
    let shared = &picker.shared;
    let Some(PendingRequest { generation, songs }) = start_request(window, shared) else {
        return;
    };
    match songs {
        PickedSongs::Ids(song_ids) => {
            let write = PlaylistWrite {
                server,
                target: name,
                songs: IdsRequest {
                    generation,
                    song_ids,
                },
            };
            create_with_ids(window, picker, write);
        }
        PickedSongs::Listed(listed) => {
            close(window, shared);
            let write = PlaylistWrite {
                server,
                target: name,
                songs: listed,
            };
            create_listed(window, &picker.context, write);
        }
    }
}

fn create_with_ids(
    window: &AppWindow,
    picker: &PlaylistPicker,
    write: PlaylistWrite<String, IdsRequest>,
) {
    let PlaylistWrite {
        server,
        target: name,
        songs: request,
    } = write;
    let song_ids = new_song_ids(&[], &request.song_ids);
    let (weak, shared) = (window.as_weak(), Arc::clone(&picker.shared));
    let views = picker.context.views(&server);
    picker.context.runtime.spawn(async move {
        let created = server.create_playlist(&name, &song_ids).await;
        if created.is_ok() {
            views.invalidate(View::Playlists).await;
        }
        on_ui(&weak, move |window| {
            let finished = FinishedCreate {
                generation: request.generation,
                song_count: song_ids.len(),
                created,
            };
            finish_create(window, &shared, finished);
        });
    });
}

struct FinishedCreate {
    generation: u64,
    song_count: usize,
    created: cadence_api::Result<Playlist>,
}

fn finish_create(window: &AppWindow, shared: &SharedPicker, finished: FinishedCreate) {
    let is_created = finished.created.is_ok();
    let outcome = match finished.created {
        Ok(playlist) => RequestOutcome::Succeeded(tr_plural!(
            "Created \"{}\" with {n} song",
            "Created \"{}\" with {n} songs",
            finished.song_count,
            playlist.name
        )),
        Err(error) => RequestOutcome::Failed(tr!("Could not create the playlist: {}", error)),
    };
    let generation = finished.generation;
    finish_request(
        window,
        shared,
        FinishedRequest {
            generation,
            outcome,
        },
    );
    if is_created {
        playlists::refresh(window);
    }
}
