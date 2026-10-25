use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::error::RemoteError;

pub const MAX_FRAME_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_HANDSHAKE_FRAME_BYTES: u64 = 16 * 1024;
const INCOMING_MESSAGE_CAPACITY: usize = 64;

pub async fn read_message<Message: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<Message>, RemoteError> {
    read_message_within(reader, MAX_FRAME_BYTES).await
}

pub async fn read_message_within<Message: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
    max_bytes: u64,
) -> Result<Option<Message>, RemoteError> {
    let mut line = Vec::new();
    let read = (&mut *reader)
        .take(max_bytes + 1)
        .read_until(b'\n', &mut line)
        .await?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() != Some(&b'\n') {
        return Err(RemoteError::Protocol(match read as u64 > max_bytes {
            true => "a message is too large",
            false => "the connection ended in the middle of a message",
        }));
    }
    Ok(Some(serde_json::from_slice(&line)?))
}

pub async fn read_handshake_message<Message: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
    timeout: Duration,
) -> Result<Option<Message>, RemoteError> {
    tokio::time::timeout(
        timeout,
        read_message_within(reader, MAX_HANDSHAKE_FRAME_BYTES),
    )
    .await
    .map_err(|_| RemoteError::TimedOut)?
}

pub fn spawn_reader<Message: DeserializeOwned + Send + 'static>(
    reader: impl AsyncBufRead + Unpin + Send + 'static,
    peer: &'static str,
) -> mpsc::Receiver<Message> {
    let (sender, receiver) = mpsc::channel(INCOMING_MESSAGE_CAPACITY);
    tokio::spawn(forward_messages(reader, sender, peer));
    receiver
}

async fn forward_messages<Message: DeserializeOwned>(
    mut reader: impl AsyncBufRead + Unpin,
    sender: mpsc::Sender<Message>,
    peer: &'static str,
) {
    loop {
        let message = match read_message::<Message>(&mut reader).await {
            Ok(Some(message)) => message,
            Ok(None) => return,
            Err(error) => {
                tracing::debug!("reading from {peer} failed: {error}");
                return;
            }
        };
        if sender.send(message).await.is_err() {
            return;
        }
    }
}

pub async fn write_message<Message: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &Message,
) -> Result<(), RemoteError> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    Ok(writer.flush().await?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::io::BufReader;

    use super::*;
    use crate::protocol::ClientMessage;

    #[tokio::test]
    async fn messages_are_one_json_line_each() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, &ClientMessage::PullPlayback)
            .await
            .unwrap();
        write_message(&mut buffer, &ClientMessage::PullPlayback)
            .await
            .unwrap();
        assert_eq!(buffer.iter().filter(|byte| **byte == b'\n').count(), 2);
        let mut reader = BufReader::new(buffer.as_slice());
        for _ in 0..2 {
            let message: Option<ClientMessage> = read_message(&mut reader).await.unwrap();
            assert_eq!(message, Some(ClientMessage::PullPlayback));
        }
        let end: Option<ClientMessage> = read_message(&mut reader).await.unwrap();
        assert_eq!(end, None);
    }

    #[tokio::test]
    async fn cut_and_invalid_lines_are_protocol_errors() {
        let mut cut = BufReader::new(b"{\"type\":\"pull_playback\"".as_slice());
        assert!(matches!(
            read_message::<ClientMessage>(&mut cut).await,
            Err(RemoteError::Protocol(_))
        ));
        let mut invalid = BufReader::new(b"not json\n".as_slice());
        assert!(matches!(
            read_message::<ClientMessage>(&mut invalid).await,
            Err(RemoteError::Malformed(_))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn given_a_silent_peer_when_reading_a_handshake_then_it_times_out() {
        let (_peer, local) = tokio::io::duplex(64);
        let mut reader = BufReader::new(local);
        let read =
            read_handshake_message::<ClientMessage>(&mut reader, Duration::from_secs(1)).await;
        assert!(matches!(read, Err(RemoteError::TimedOut)));
    }

    #[tokio::test]
    async fn given_two_messages_when_spawning_a_reader_then_both_arrive_and_the_channel_closes() {
        let mut buffer = Vec::new();
        for _ in 0..2 {
            write_message(&mut buffer, &ClientMessage::PullPlayback)
                .await
                .unwrap();
        }
        let mut incoming =
            spawn_reader::<ClientMessage>(BufReader::new(Cursor::new(buffer)), "a test peer");
        assert_eq!(incoming.recv().await, Some(ClientMessage::PullPlayback));
        assert_eq!(incoming.recv().await, Some(ClientMessage::PullPlayback));
        assert_eq!(incoming.recv().await, None);
    }
}
