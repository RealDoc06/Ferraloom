//! Socket ownership and the handshake/status/login connection state machine.

use std::{
    io,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicI32, AtomicUsize, Ordering},
    },
    time::Duration,
};

use md5::{Digest, Md5};
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};

use crate::{
    MINECRAFT_VERSION, PROTOCOL_VERSION,
    protocol::{self, Packet, Reader, put_string, put_var_int, write_packet},
};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub offline: bool,
    pub max_connections: usize,
    pub max_players: u32,
    pub motd: String,
    pub packet_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            offline: false,
            max_connections: 256,
            max_players: 20,
            motd: "Ferraloom independent engine (in development)".into(),
            packet_timeout: Duration::from_secs(15),
        }
    }
}

pub struct Server {
    listener: TcpListener,
    config: Arc<ServerConfig>,
    state: Arc<ServerState>,
}

pub(crate) struct ServerState {
    players: Arc<Semaphore>,
    online: AtomicUsize,
    next_entity: AtomicI32,
}

impl ServerState {
    pub(crate) fn new(max_players: u32) -> Self {
        Self {
            players: Arc::new(Semaphore::new(max_players as usize)),
            online: AtomicUsize::new(0),
            next_entity: AtomicI32::new(1),
        }
    }
    pub(crate) fn allocate_entity(&self) -> io::Result<i32> {
        self.next_entity
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| io::Error::other("entity identifiers exhausted"))
    }
    pub(crate) fn admit_player(&self) -> Option<PlayerSession<'_>> {
        let permit = Arc::clone(&self.players).try_acquire_owned().ok()?;
        self.online.fetch_add(1, Ordering::Relaxed);
        Some(PlayerSession {
            _permit: permit,
            online: &self.online,
        })
    }
}

pub(crate) struct PlayerSession<'a> {
    _permit: tokio::sync::OwnedSemaphorePermit,
    online: &'a AtomicUsize,
}

impl Drop for PlayerSession<'_> {
    fn drop(&mut self) {
        self.online.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Server {
    pub async fn bind(address: SocketAddr, config: ServerConfig) -> io::Result<Self> {
        if config.max_connections == 0
            || config.max_connections > Semaphore::MAX_PERMITS
            || config.packet_timeout.is_zero()
            || config.max_players == 0
            || config.max_players > i32::MAX as u32
            || config.max_players as usize > Semaphore::MAX_PERMITS
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "connection limit and timeout must be positive",
            ));
        }
        Ok(Self {
            listener: TcpListener::bind(address).await?,
            state: Arc::new(ServerState::new(config.max_players)),
            config: Arc::new(config),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> io::Result<()> {
        let permits = Arc::new(Semaphore::new(self.config.max_connections));
        let mut connections = JoinSet::new();
        let result = loop {
            if *shutdown.borrow() {
                break Ok(());
            }
            tokio::select! {
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() { break Ok(()); }
                }
                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    if let Err(error) = result { break Err(io::Error::other(error)); }
                }
                accepted = self.listener.accept() => {
                    let (socket, _) = match accepted { Ok(value) => value, Err(error) => break Err(error) };
                    let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else { continue; };
                    let config = Arc::clone(&self.config);
                    let state = Arc::clone(&self.state);
                    let mut stop = shutdown.clone();
                    connections.spawn(async move {
                        let _permit = permit;
                        tokio::select! {
                            result = serve(socket, &config, &state) => {
                                // Malformed/disconnected peers are isolated to their connection.
                                if let Err(error) = result
                                    && !matches!(error.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe | io::ErrorKind::InvalidData | io::ErrorKind::TimedOut) {
                                        eprintln!("connection: {error}");
                                }
                            }
                            _ = stop.changed() => {}
                        }
                    });
                }
            }
        };
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        result
    }
}

async fn serve(socket: TcpStream, config: &ServerConfig, state: &ServerState) -> io::Result<()> {
    socket.set_nodelay(true)?;
    serve_connection(socket, config, state).await
}

pub(crate) async fn next_packet(
    stream: &mut (impl AsyncRead + Unpin),
    config: &ServerConfig,
) -> io::Result<Packet> {
    timeout(config.packet_timeout, protocol::read_packet(stream))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "packet timed out"))??
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "peer closed"))
}

fn malformed(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

async fn serve_connection(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    config: &ServerConfig,
    state: &ServerState,
) -> io::Result<()> {
    let handshake = next_packet(&mut stream, config).await?;
    if handshake.id != 0 {
        return Err(malformed("expected handshake"));
    }
    let mut fields = Reader::new(&handshake.payload);
    let version = fields.var_int()?;
    let _address = fields.string(255)?;
    let _port = fields.unsigned_short()?;
    let intent = fields.var_int()?;
    fields.finish()?;
    match intent {
        1 => status(&mut stream, config, state).await?,
        2 => login(&mut stream, config, state, version).await?,
        _ => return Err(malformed("unsupported connection intent")),
    }
    stream.shutdown().await
}

async fn status(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    config: &ServerConfig,
    state: &ServerState,
) -> io::Result<()> {
    let request = next_packet(stream, config).await?;
    if request.id != 0 || !request.payload.is_empty() {
        return Err(malformed("expected status request"));
    }
    let response = serde_json::json!({
        "version": { "name": MINECRAFT_VERSION, "protocol": PROTOCOL_VERSION },
        "players": { "max": config.max_players, "online": state.online.load(Ordering::Relaxed), "sample": [] },
        "description": { "text": config.motd },
        "enforcesSecureChat": false,
    })
    .to_string();
    let mut payload = Vec::new();
    put_string(&mut payload, &response)?;
    write_packet(stream, 0, &payload).await?;
    let ping = next_packet(stream, config).await?;
    if ping.id != 1 || ping.payload.len() != 8 {
        return Err(malformed("expected status ping"));
    }
    write_packet(stream, 1, &ping.payload).await
}

/// Java's UUID.nameUUIDFromBytes("OfflinePlayer:" + name).
pub fn offline_uuid(name: &str) -> [u8; 16] {
    let mut hash = Md5::new();
    hash.update(b"OfflinePlayer:");
    hash.update(name.as_bytes());
    let mut bytes: [u8; 16] = hash.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

async fn login(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    config: &ServerConfig,
    state: &ServerState,
    version: i32,
) -> io::Result<()> {
    if version != PROTOCOL_VERSION {
        return login_disconnect(stream, "This server requires Minecraft Java 26.3").await;
    }
    if !config.offline {
        return login_disconnect(
            stream,
            "Account authentication is not implemented yet; isolated testing requires --offline",
        )
        .await;
    }
    let hello = next_packet(stream, config).await?;
    if hello.id != 0 {
        return Err(malformed("expected login hello"));
    }
    let mut reader = Reader::new(&hello.payload);
    let name = reader.string(16)?;
    let _claimed_uuid = reader.take(16)?;
    reader.finish()?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return login_disconnect(stream, "Invalid player name").await;
    }
    let uuid = offline_uuid(name);
    let mut profile = Vec::new();
    profile.extend_from_slice(&uuid);
    put_string(&mut profile, name)?;
    put_var_int(&mut profile, 0); // No unauthenticated profile properties.
    // 26.3 LoginFinished carries a session UUID after the GameProfile.
    // It identifies this login session, separately from the stable player UUID.
    profile.extend_from_slice(&session_uuid()?);
    write_packet(stream, 2, &profile).await?;
    let acknowledgement = next_packet(stream, config).await?;
    if acknowledgement.id != 3 || !acknowledgement.payload.is_empty() {
        return Err(malformed("expected login acknowledgement"));
    }
    let configured = timeout(
        config.packet_timeout * 4,
        crate::configuration::configure(stream, config),
    )
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "configuration timed out"))??;
    if !configured {
        return Ok(());
    }
    crate::play::enter(stream, config, state).await
}

fn session_uuid() -> io::Result<[u8; 16]> {
    let mut uuid = [0u8; 16];
    getrandom::fill(&mut uuid).map_err(|error| io::Error::other(error.to_string()))?;
    uuid[6] = (uuid[6] & 0x0f) | 0x40;
    uuid[8] = (uuid[8] & 0x3f) | 0x80;
    Ok(uuid)
}

async fn login_disconnect(stream: &mut (impl AsyncWrite + Unpin), reason: &str) -> io::Result<()> {
    let mut payload = Vec::new();
    put_string(
        &mut payload,
        &serde_json::json!({"text":reason}).to_string(),
    )?;
    write_packet(stream, 0, &payload).await
}

#[cfg(test)]
mod tests;
