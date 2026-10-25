#![cfg(test)]

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cadence_platform::{
    DiscordDisplayType, DiscordOptions, DiscordPresence, NowPlaying, PlatformError,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const RECEIVE_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_OPCODE: u32 = 0;
const FRAME_OPCODE: u32 = 1;
const CLOSE_OPCODE: u32 = 2;
const LISTENING_ACTIVITY_TYPE: u32 = 2;
const ARTIST_STATUS_DISPLAY_TYPE: u32 = 1;

static DISCORD_ENVIRONMENT: Mutex<()> = Mutex::new(());
static SYSTEM_TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();

type Frame = (u32, Value);

enum Greeting {
    Ready,
    HangUp,
}

enum Reply {
    Accepted,
    Rejected,
    Closed,
    AcceptedWhenReleased(mpsc::Receiver<()>),
}

struct Session {
    greeting: Greeting,
    replies: Vec<Reply>,
}

impl Session {
    fn answering(replies: Vec<Reply>) -> Self {
        Self {
            greeting: Greeting::Ready,
            replies,
        }
    }

    fn hanging_up() -> Self {
        Self {
            greeting: Greeting::HangUp,
            replies: Vec::new(),
        }
    }
}

struct PointedEnvironment {
    runtime_dir: TempDir,
    _lock: MutexGuard<'static, ()>,
}

struct MockDiscord {
    _environment: PointedEnvironment,
    server: Option<JoinHandle<()>>,
    frames: mpsc::Receiver<Frame>,
}

impl MockDiscord {
    fn next_frame(&self) -> Frame {
        self.frames.recv_timeout(RECEIVE_TIMEOUT).unwrap()
    }

    fn next_handshake(&self) {
        let (opcode, handshake) = self.next_frame();
        assert_eq!(opcode, HANDSHAKE_OPCODE);
        assert_eq!(handshake, json!({ "v": 1, "client_id": "123" }));
    }

    fn next_activity(&self) -> Value {
        let (opcode, frame) = self.next_frame();
        assert_eq!(opcode, FRAME_OPCODE);
        assert_eq!(frame["cmd"], "SET_ACTIVITY");
        frame["args"]["activity"].clone()
    }

    fn finish(mut self, presence: DiscordPresence) {
        drop(presence);
        let (opcode, _) = self.next_frame();
        assert_eq!(opcode, CLOSE_OPCODE);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

fn read_frame(stream: &mut UnixStream) -> Option<Frame> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header).ok()?;
    let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
    let body_length = u32::from_le_bytes(header[4..].try_into().unwrap());
    let mut body = vec![0u8; body_length as usize];
    stream.read_exact(&mut body).ok()?;
    Some((opcode, serde_json::from_slice(&body).unwrap()))
}

fn write_frame(stream: &mut UnixStream, opcode: u32, value: &Value) {
    let body = value.to_string();
    stream.write_all(&opcode.to_le_bytes()).unwrap();
    stream
        .write_all(&u32::try_from(body.len()).unwrap().to_le_bytes())
        .unwrap();
    stream.write_all(body.as_bytes()).unwrap();
}

fn point_discord_ipc_at(runtime_dir: &Path) {
    unsafe {
        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            std::env::set_var(key, runtime_dir);
        }
        std::env::remove_var("SNAP");
    }
}

fn pointed_environment() -> PointedEnvironment {
    let lock = DISCORD_ENVIRONMENT
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let system_temp_dir = SYSTEM_TEMP_DIR.get_or_init(std::env::temp_dir);
    let runtime_dir = tempfile::tempdir_in(system_temp_dir).unwrap();
    point_discord_ipc_at(runtime_dir.path());
    PointedEnvironment {
        runtime_dir,
        _lock: lock,
    }
}

fn reply_to(stream: &mut UnixStream, nonce: Option<Value>, reply: Reply) -> bool {
    let event_name = match reply {
        Reply::Closed => {
            write_frame(stream, CLOSE_OPCODE, &json!({ "code": 4000 }));
            return false;
        }
        Reply::Rejected => json!("ERROR"),
        Reply::Accepted => Value::Null,
        Reply::AcceptedWhenReleased(release) => {
            release.recv_timeout(RECEIVE_TIMEOUT).unwrap();
            Value::Null
        }
    };
    write_frame(
        stream,
        FRAME_OPCODE,
        &json!({ "cmd": "SET_ACTIVITY", "evt": event_name, "nonce": nonce, "data": {} }),
    );
    true
}

fn answer_frames(mut stream: UnixStream, session: Session, frame_sender: &mpsc::Sender<Frame>) {
    let Some(handshake) = read_frame(&mut stream) else {
        return;
    };
    frame_sender.send(handshake).unwrap();
    if let Greeting::HangUp = session.greeting {
        return;
    }
    write_frame(
        &mut stream,
        FRAME_OPCODE,
        &json!({ "cmd": "DISPATCH", "evt": "READY", "data": { "v": 1 } }),
    );
    let mut replies = session.replies.into_iter();
    while let Some((opcode, frame)) = read_frame(&mut stream) {
        let nonce = frame.get("nonce").cloned();
        frame_sender.send((opcode, frame)).unwrap();
        if opcode == CLOSE_OPCODE {
            return;
        }
        if !reply_to(
            &mut stream,
            nonce,
            replies.next().unwrap_or(Reply::Accepted),
        ) {
            return;
        }
    }
}

fn spawn_mock_discord(sessions: Vec<Session>) -> MockDiscord {
    let environment = pointed_environment();
    let listener =
        UnixListener::bind(environment.runtime_dir.path().join("discord-ipc-0")).unwrap();
    let (frame_sender, frames) = mpsc::channel();
    let server = thread::spawn(move || {
        for session in sessions {
            let (stream, _) = listener.accept().unwrap();
            answer_frames(stream, session, &frame_sender);
        }
    });
    MockDiscord {
        _environment: environment,
        server: Some(server),
        frames,
    }
}

fn connected(sessions: Vec<Session>) -> (MockDiscord, DiscordPresence) {
    let discord = spawn_mock_discord(sessions);
    let presence = DiscordPresence::connect("123").unwrap();
    discord.next_handshake();
    (discord, presence)
}

fn now_playing() -> NowPlaying {
    NowPlaying {
        track_id: "t1".to_owned(),
        title: "Song".to_owned(),
        artists: vec!["Artist".to_owned()],
        album: Some("Album".to_owned()),
        duration_ms: Some(200_000),
        cover_url: Some("https://example.com/cover.jpg".to_owned()),
        cover_file: None,
    }
}

fn song_titled(title: &str) -> NowPlaying {
    NowPlaying {
        track_id: format!("track-{title}"),
        title: title.to_owned(),
        ..now_playing()
    }
}

fn assert_song_activity(activity: &Value) {
    assert_eq!(activity["type"], LISTENING_ACTIVITY_TYPE);
    assert_eq!(activity["details"], "Song");
    assert_eq!(activity["state"], "Artist");
    assert_eq!(
        activity["assets"]["large_image"],
        "https://example.com/cover.jpg"
    );
    assert_eq!(activity["assets"]["large_text"], "Album");
    let start = activity["timestamps"]["start"].as_i64().unwrap();
    let end = activity["timestamps"]["end"].as_i64().unwrap();
    assert_eq!(end - start, 200_000);
}

#[test]
fn discord_presence_over_mock_ipc() {
    {
        let _environment = pointed_environment();
        assert!(matches!(
            DiscordPresence::connect("123"),
            Err(PlatformError::Discord(_))
        ));
    }

    let (discord, mut presence) = connected(vec![Session::answering(Vec::new())]);

    presence
        .set_activity(&now_playing(), Some(30_000), false)
        .unwrap();
    assert_song_activity(&discord.next_activity());

    presence.clear().unwrap();
    assert_eq!(discord.next_activity(), Value::Null);

    discord.finish(presence);
}

#[test]
fn given_discord_rejects_an_activity_when_updating_again_then_the_same_connection_is_used() {
    let (discord, mut presence) = connected(vec![Session::answering(vec![Reply::Rejected])]);

    presence
        .set_activity(&now_playing(), Some(0), false)
        .unwrap();
    assert_song_activity(&discord.next_activity());

    presence.clear().unwrap();
    assert_eq!(discord.next_activity(), Value::Null);

    discord.finish(presence);
}

#[test]
fn given_discord_closes_the_connection_when_updating_then_the_update_is_resent_after_reconnecting()
{
    let (discord, mut presence) = connected(vec![
        Session::answering(vec![Reply::Closed]),
        Session::answering(Vec::new()),
    ]);

    presence
        .set_activity(&now_playing(), Some(0), false)
        .unwrap();
    let refused = discord.next_activity();
    discord.next_handshake();
    let resent = discord.next_activity();
    assert_song_activity(&resent);
    assert_eq!(resent, refused);

    discord.finish(presence);
}

#[test]
fn given_discord_cannot_be_reached_again_when_the_next_update_is_sent_then_it_reconnects() {
    let (discord, mut presence) = connected(vec![
        Session::answering(vec![Reply::Closed]),
        Session::hanging_up(),
        Session::answering(Vec::new()),
    ]);

    presence
        .set_activity(&now_playing(), Some(0), false)
        .unwrap();
    assert_song_activity(&discord.next_activity());
    discord.next_handshake();

    presence.clear().unwrap();
    discord.next_handshake();
    assert_eq!(discord.next_activity(), Value::Null);

    discord.finish(presence);
}

#[test]
fn given_updates_queue_up_while_discord_answers_when_it_answers_then_only_the_latest_is_sent() {
    let (release_sender, release) = mpsc::channel();
    let (discord, mut presence) =
        connected(vec![Session::answering(vec![Reply::AcceptedWhenReleased(
            release,
        )])]);

    presence
        .set_activity(&now_playing(), Some(0), false)
        .unwrap();
    assert_song_activity(&discord.next_activity());
    presence
        .set_activity(&song_titled("Skipped"), Some(0), false)
        .unwrap();
    presence.clear().unwrap();
    presence
        .set_activity(&song_titled("Latest"), Some(0), false)
        .unwrap();
    release_sender.send(()).unwrap();

    assert_eq!(discord.next_activity()["details"], "Latest");

    discord.finish(presence);
}

#[test]
fn given_paused_songs_are_hidden_when_the_song_pauses_then_the_activity_is_cleared() {
    let (discord, mut presence) = connected(vec![Session::answering(Vec::new())]);
    let options = DiscordOptions {
        display_type: DiscordDisplayType::ArtistName,
        is_shown_when_paused: false,
        ..DiscordOptions::default()
    };
    presence.set_options(options.clone());
    assert_eq!(presence.options(), &options);

    presence
        .set_activity(&now_playing(), Some(0), false)
        .unwrap();
    assert_eq!(
        discord.next_activity()["status_display_type"],
        ARTIST_STATUS_DISPLAY_TYPE
    );

    presence
        .set_activity(&now_playing(), Some(0), true)
        .unwrap();
    assert_eq!(discord.next_activity(), Value::Null);

    discord.finish(presence);
}
