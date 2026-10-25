use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;

use crate::error::RemoteError;
use crate::frame::{read_handshake_message, spawn_reader, write_message};
use crate::pairing::{Nonce, Proof, proof_of};
use crate::protocol::{ClientMessage, DeviceInfo, HostMessage, PROTOCOL_VERSION, ServerKey};
use crate::trust::TrustStore;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CHALLENGE_TIMEOUT: Duration = Duration::from_secs(10);
const APPROVAL_WAIT: Duration = Duration::from_secs(75);
const REMOTE_PLAYER: &str = "a remote player";

pub struct ClientLink {
    pub host: DeviceInfo,
    pub server: Option<ServerKey>,
    pub outgoing: mpsc::UnboundedSender<ClientMessage>,
    pub incoming: mpsc::Receiver<HostMessage>,
}

pub async fn connect(
    address: SocketAddr,
    me: DeviceInfo,
    trust: Arc<dyn TrustStore>,
) -> Result<ClientLink, RemoteError> {
    let stream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            tracing::info!("connecting to the remote player at {address} failed: {error}");
            return Err(RemoteError::Unreachable);
        }
        Err(_) => {
            tracing::info!("the remote player at {address} did not accept the connection");
            return Err(RemoteError::Unreachable);
        }
    };
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let (host, nonce) = read_challenge(&mut reader).await?;
    let proof = proof_for(&host, &nonce, trust.as_ref());
    send_hello(&mut writer, me, proof).await?;
    let server = await_welcome(&mut reader, &host, trust.as_ref()).await?;
    Ok(ClientLink {
        host,
        server,
        outgoing: spawn_writer(writer),
        incoming: spawn_reader(reader, REMOTE_PLAYER),
    })
}

async fn read_challenge(
    reader: &mut BufReader<OwnedReadHalf>,
) -> Result<(DeviceInfo, Nonce), RemoteError> {
    let challenge = read_handshake_message(reader, CHALLENGE_TIMEOUT).await?;
    let Some(HostMessage::Challenge {
        version,
        device: host,
        nonce,
    }) = challenge
    else {
        return Err(RemoteError::Protocol("expected a challenge"));
    };
    let host = host.cleaned();
    if version != PROTOCOL_VERSION {
        return Err(RemoteError::Failed(
            crate::protocol::RemoteFailure::VersionMismatch,
        ));
    }
    Ok((host, nonce))
}

fn proof_for(host: &DeviceInfo, nonce: &Nonce, trust: &dyn TrustStore) -> Option<Proof> {
    trust
        .host_token(&host.id)
        .map(|token| proof_of(&token, nonce))
}

async fn send_hello(
    writer: &mut OwnedWriteHalf,
    me: DeviceInfo,
    proof: Option<Proof>,
) -> Result<(), RemoteError> {
    let hello = ClientMessage::Hello {
        version: PROTOCOL_VERSION,
        device: me,
        proof,
    };
    write_message(writer, &hello).await
}

async fn await_welcome(
    reader: &mut BufReader<OwnedReadHalf>,
    host: &DeviceInfo,
    trust: &dyn TrustStore,
) -> Result<Option<ServerKey>, RemoteError> {
    loop {
        let answer = read_handshake_message(reader, APPROVAL_WAIT).await?;
        match answer {
            Some(HostMessage::Paired { token }) => trust.remember_host(&host.id, token),
            Some(HostMessage::Welcome { server }) => return Ok(server),
            Some(HostMessage::Denied) => return Err(RemoteError::Denied),
            Some(HostMessage::Failed { failure }) => return Err(RemoteError::Failed(failure)),
            Some(_) => return Err(RemoteError::Protocol("expected a welcome")),
            None => return Err(connection_closed()),
        }
    }
}

fn spawn_writer(mut writer: OwnedWriteHalf) -> mpsc::UnboundedSender<ClientMessage> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<ClientMessage>();
    tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            if let Err(error) = write_message(&mut writer, &message).await {
                tracing::debug!("sending to a remote player failed: {error}");
                return;
            }
        }
    });
    sender
}

fn connection_closed() -> RemoteError {
    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the connection closed").into()
}
