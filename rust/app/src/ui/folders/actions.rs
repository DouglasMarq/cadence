use cadence_core::model::FolderId;
use slint::ComponentHandle;

use super::binding::with_model;
use crate::controller::Command;
use crate::list_stream::{ListRow, Placement, RowRanges, SongSource};
use crate::ui::{AppWindow, Context, FolderPlayMode, FolderSongAction, PlayerState};

pub fn folder_play_commands(
    window: &AppWindow,
    folder_id: &str,
    mode: FolderPlayMode,
) -> Vec<Command> {
    let source = SongSource::FolderTree(FolderId::from_detail_id(folder_id));
    let enqueue = |placement| Command::EnqueueFromList {
        source: source.clone(),
        ranges: vec![RowRanges::WHOLE_LIST],
        placement,
    };
    match mode {
        FolderPlayMode::Now => vec![Command::PlayFromList {
            source: source.clone(),
            song_id: None,
            row_index: ListRow(0),
        }],
        FolderPlayMode::Shuffle => {
            let is_shuffled = window.global::<PlayerState>().get_is_shuffled();
            let toggle = (!is_shuffled).then_some(Command::ToggleShuffle);
            toggle
                .into_iter()
                .chain([enqueue(Placement::Now)])
                .collect()
        }
        FolderPlayMode::Next => vec![enqueue(Placement::Next)],
        FolderPlayMode::Last => vec![enqueue(Placement::Last)],
    }
}

pub fn play_row(window: &AppWindow, context: &Context, row: usize) {
    let Some((folder, (position, song))) =
        with_model(window, |model| Some((model.folder(), model.song_at(row)?))).flatten()
    else {
        return;
    };
    context.send(Command::PlayFromList {
        source: SongSource::Folder(folder),
        song_id: Some(song.id),
        row_index: ListRow(position),
    });
}

pub fn song_action_command(
    window: &AppWindow,
    row: usize,
    action: FolderSongAction,
) -> Option<Command> {
    let (_, song) = with_model(window, |model| model.song_at(row)).flatten()?;
    Some(match action {
        FolderSongAction::PlayNext => Command::AddNext(vec![song]),
        FolderSongAction::AddToQueue => Command::AddLast(vec![song]),
    })
}
