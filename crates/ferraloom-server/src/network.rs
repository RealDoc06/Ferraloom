//! Socket ownership stays here. Only typed, FIFO ingress reaches the tick thread.
use crate::properties::ServerProperties;
use anyhow::{Context, Result, bail, ensure};
use ferraloom_protocol::{
    self as protocol, Uuid, Wire,
    chat::{Component, ProfileProperty},
    common, configuration, handshake, login, play, status,
    transport::*,
};
use rand::RngCore;
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, watch},
};

pub type ConnectionId = u64;
/// Accepted on the tick thread in channel order; connection sequence never resets.
pub enum Ingress {
    Admission {
        id: ConnectionId,
        profile: login::GameProfile,
        address: SocketAddr,
        reply: oneshot::Sender<Option<String>>,
    },
    Connected {
        id: ConnectionId,
        profile: login::GameProfile,
        address: SocketAddr,
        outbound: mpsc::Sender<OutboundBatch>,
    },
    Packet {
        id: ConnectionId,
        sequence: u64,
        packet: Incoming,
    },
    Closed {
        id: ConnectionId,
    },
    Console(String),
}
#[derive(Debug)]
pub enum Incoming {
    Configuration(configuration::ServerboundPacket),
    Play(Box<play::serverbound::Packet>),
}
impl Incoming {
    /// ServerGamePacketListenerImpl#tryHandleChat uses server.execute instead
    /// of PacketUtils' pre-tick PacketProcessor. The game handler is unchanged.
    pub(crate) fn uses_main_executor(&self) -> bool {
        matches!(self, Self::Play(packet) if matches!(packet.as_ref(),
            play::serverbound::Packet::Chat(_)
                | play::serverbound::Packet::ChatCommand(_)
                | play::serverbound::Packet::ChatCommandSigned(_)))
    }
}

#[derive(Clone, Debug)]
pub enum Outgoing {
    Configuration(Box<configuration::ClientboundPacket>),
    Play(Box<play::clientbound::Packet>),
}
impl Outgoing {
    fn bytes(&self, registries: &crate::registries::RegistrySet) -> Result<Vec<u8>> {
        let mut writer = protocol::Writer::with_context(registries);
        match self {
            Self::Configuration(p) => p.encode(&mut writer)?,
            Self::Play(p) => p.encode(&mut writer)?,
        }
        Ok(writer.into_inner())
    }
}
impl From<play::clientbound::Packet> for Outgoing {
    fn from(p: play::clientbound::Packet) -> Self {
        Self::Play(Box::new(p))
    }
}
impl From<configuration::ClientboundPacket> for Outgoing {
    fn from(p: configuration::ClientboundPacket) -> Self {
        Self::Configuration(Box::new(p))
    }
}
pub struct OutboundBatch {
    pub sequence: u64,
    pub packets: Vec<Outgoing>,
    pub close: bool,
}

/// Port of Connection's framing, compression and stream cipher pipeline.
pub struct PacketIo<T> {
    pub socket: T,
    decode: CompressionDecoder,
    encode: CompressionEncoder,
    decrypt: Option<CipherDecoder>,
    encrypt: Option<CipherEncoder>,
}
impl<T: AsyncRead + AsyncWrite + Unpin> PacketIo<T> {
    pub fn new(socket: T) -> Self {
        Self {
            socket,
            decode: CompressionDecoder::new(-1, true),
            encode: CompressionEncoder::new(-1),
            decrypt: None,
            encrypt: None,
        }
    }
    pub fn compression(&mut self, threshold: i32) {
        self.decode.set_threshold(threshold, true);
        self.encode.set_threshold(threshold);
    }
    fn encryption(&mut self, secret: &[u8; 16]) {
        self.decrypt = Some(CipherDecoder::new(secret));
        self.encrypt = Some(CipherEncoder::new(secret));
    }
    pub async fn read(&mut self) -> Result<Vec<u8>> {
        read_frame(&mut self.socket, &mut self.decode, &mut self.decrypt).await
    }
    pub async fn send(&mut self, packet: &impl Wire) -> Result<()> {
        write_frame(
            &mut self.socket,
            &mut self.encode,
            &mut self.encrypt,
            &packet.to_bytes()?,
        )
        .await
    }
}
async fn read_frame<T: AsyncRead + Unpin>(
    socket: &mut T,
    compression: &mut CompressionDecoder,
    cipher: &mut Option<CipherDecoder>,
) -> Result<Vec<u8>> {
    let mut length = 0usize;
    for index in 0..3 {
        let mut b = [socket.read_u8().await?];
        if let Some(cipher) = cipher {
            cipher.decrypt(&mut b);
        }
        length |= usize::from(b[0] & 127) << (index * 7);
        if b[0] & 128 == 0 {
            ensure!(length > 0, "zero packet length");
            let mut frame = vec![0; length];
            socket.read_exact(&mut frame).await?;
            if let Some(cipher) = cipher {
                cipher.decrypt(&mut frame);
            }
            return Ok(compression.decode(&frame)?.into_owned());
        }
    }
    bail!("packet length wider than 21 bits")
}
async fn write_frame<T: AsyncWrite + Unpin>(
    socket: &mut T,
    compression: &mut CompressionEncoder,
    cipher: &mut Option<CipherEncoder>,
    packet: &[u8],
) -> Result<()> {
    let mut bytes = FrameEncoder::encode(&compression.encode(packet)?)?;
    if let Some(cipher) = cipher {
        cipher.encrypt(&mut bytes);
    }
    socket.write_all(&bytes).await?;
    Ok(())
}

pub struct NetworkOptions {
    pub properties: Arc<ServerProperties>,
    pub keys: LoginKeyPair,
    pub session_id: Uuid,
    pub status: watch::Receiver<status::ServerStatus>,
    pub registries: Arc<crate::registries::RegistrySet>,
    pub encoding: Arc<crate::jobs::JobPool>,
}
/// Port of ServerConnectionListener. The listener only touches its bound socket.
pub async fn listen(
    listener: TcpListener,
    options: Arc<NetworkOptions>,
    ingress: mpsc::Sender<Ingress>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut next_id = 0;
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            _=connections.join_next(), if !connections.is_empty()=>{},
            changed=shutdown.changed()=> { if changed.is_err() || *shutdown.borrow() {break;} },
            accepted=listener.accept()=> {
                let (socket,address)=accepted?;socket.set_nodelay(true)?;
                let id=next_id;next_id+=1;
                let options=options.clone();let ingress=ingress.clone();
                connections.spawn(async move {
                    if let Err(e)=connection(socket,address,id,options,ingress.clone()).await { eprintln!("Connection {id} ({address}): {e:#}"); }
                    let _=ingress.send(Ingress::Closed{id}).await;
                });
            }
        }
    }
    if tokio::time::timeout(Duration::from_secs(3), async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}
async fn connection(
    mut socket: TcpStream,
    address: SocketAddr,
    id: ConnectionId,
    options: Arc<NetworkOptions>,
    ingress: mpsc::Sender<Ingress>,
) -> Result<()> {
    let mut first = [0; 512];
    let count = tokio::time::timeout(Duration::from_secs(30), socket.peek(&mut first)).await??;
    if count == 0 {
        return Ok(());
    }
    if first[0] == 0xfe {
        let snapshot = options.status.borrow().clone();
        legacy_ping(&mut socket, &snapshot, &first[..count]).await?;
        return Ok(());
    }
    let mut io = PacketIo::new(socket);
    let handshake::ServerboundPacket::Intention(hello) =
        handshake::ServerboundPacket::from_bytes(&io.read().await?)?;
    if hello.intention == handshake::ClientIntent::Status {
        if !options.properties.boolean("enable-status") {
            return Ok(());
        }
        let packet = status::ServerboundPacket::from_bytes(&io.read().await?)?;
        ensure!(
            matches!(packet, status::ServerboundPacket::Request(_)),
            "expected status request"
        );
        let snapshot = options.status.borrow().clone();
        io.send(&status::ClientboundPacket::Response(Box::new(
            status::StatusResponse { status: snapshot },
        )))
        .await?;
        if let status::ServerboundPacket::Ping(p) =
            status::ServerboundPacket::from_bytes(&io.read().await?)?
        {
            io.send(&status::ClientboundPacket::Pong(
                common::clientbound::PongResponse { time: p.time },
            ))
            .await?;
        }
        return Ok(());
    }
    if hello.intention == handshake::ClientIntent::Transfer
        && !options.properties.boolean("accepts-transfers")
    {
        return login_disconnect(&mut io, "multiplayer.disconnect.transfers_disabled").await;
    }
    if hello.protocol_version != protocol::PROTOCOL_VERSION {
        return login_disconnect(&mut io, "multiplayer.disconnect.outdated_client").await;
    }
    let profile = match tokio::time::timeout(
        Duration::from_secs(30),
        authenticate(&mut io, address, &options),
    )
    .await
    {
        Ok(Ok(profile)) => profile,
        Ok(Err(e)) => {
            let _ = login_disconnect(&mut io, &e.to_string()).await;
            return Err(e);
        }
        Err(_) => return login_disconnect(&mut io, "multiplayer.disconnect.slow_login").await,
    };
    let (reply, answer) = oneshot::channel();
    ingress
        .send(Ingress::Admission {
            id,
            profile: profile.clone(),
            address,
            reply,
        })
        .await?;
    if let Some(reason) = answer.await? {
        return login_disconnect(&mut io, &reason).await;
    }
    let threshold = options.properties.int("network-compression-threshold");
    if threshold >= 0 {
        io.send(&login::ClientboundPacket::Compression(
            login::LoginCompression {
                threshold: protocol::codec::VarInt(threshold),
            },
        ))
        .await?;
        io.compression(threshold);
    }
    io.send(&login::ClientboundPacket::LoginFinished(
        login::LoginFinished {
            game_profile: profile.clone(),
            session_id: options.session_id,
        },
    ))
    .await?;
    let ack = tokio::time::timeout(Duration::from_secs(30), io.read()).await??;
    ensure!(
        matches!(
            login::ServerboundPacket::from_bytes(&ack)?,
            login::ServerboundPacket::LoginAcknowledged(_)
        ),
        "expected login acknowledgement"
    );
    let (outbound, mut batches) = mpsc::channel::<OutboundBatch>(128);
    ingress
        .send(Ingress::Connected {
            id,
            profile,
            address,
            outbound,
        })
        .await?;
    let (mut read, mut write) = io.socket.into_split();
    let mut decoder = io.decode;
    let mut decrypt = io.decrypt;
    let mut encoder = io.encode;
    let mut encrypt = io.encrypt;
    let reader = async {
        let mut configuration = true;
        let mut sequence = 0;
        loop {
            let bytes = tokio::time::timeout(
                Duration::from_secs(30),
                read_frame(&mut read, &mut decoder, &mut decrypt),
            )
            .await??;
            let packet = if configuration {
                let packet = configuration::ServerboundPacket::from_bytes(&bytes)?;
                if matches!(
                    packet,
                    configuration::ServerboundPacket::FinishConfiguration(_)
                ) {
                    configuration = false;
                }
                Incoming::Configuration(packet)
            } else {
                Incoming::Play(Box::new(decode_play(&bytes, options.registries.as_ref())?))
            };
            ingress
                .send(Ingress::Packet {
                    id,
                    sequence,
                    packet,
                })
                .await?;
            sequence += 1;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let writer = async {
        let mut expected = 0;
        while let Some(batch) = batches.recv().await {
            ensure!(batch.sequence == expected, "outbound order mismatch");
            expected += 1;
            let registries = options.registries.clone();
            let (returned, mut bytes) = options
                .encoding
                .run(move || -> Result<_> {
                    let mut bytes = Vec::new();
                    for packet in batch.packets {
                        let encoded = FrameEncoder::encode(
                            &encoder.encode(&packet.bytes(registries.as_ref())?)?,
                        )?;
                        ensure!(
                            bytes.len() + encoded.len() <= 16 * 1024 * 1024,
                            "outbound batch exceeds byte limit"
                        );
                        bytes.extend_from_slice(&encoded);
                    }
                    Ok((encoder, bytes))
                })
                .await??;
            encoder = returned;
            if let Some(cipher) = &mut encrypt {
                cipher.encrypt(&mut bytes);
            }
            write.write_all(&bytes).await?;
            write.flush().await?;
            if batch.close {
                break;
            }
        }
        write.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    };
    tokio::select! {r=reader=>r,r=writer=>r}
}
async fn login_disconnect<T: AsyncRead + AsyncWrite + Unpin>(
    io: &mut PacketIo<T>,
    reason: &str,
) -> Result<()> {
    io.send(&login::ClientboundPacket::Disconnect(Box::new(
        login::LoginDisconnect {
            reason: Component::literal(reason),
        },
    )))
    .await
}
async fn authenticate(
    io: &mut PacketIo<TcpStream>,
    address: SocketAddr,
    options: &NetworkOptions,
) -> Result<login::GameProfile> {
    let login::ServerboundPacket::Hello(hello) =
        login::ServerboundPacket::from_bytes(&io.read().await?)?
    else {
        bail!("expected login hello");
    };
    ensure!(
        !hello.name.is_empty()
            && hello
                .name
                .chars()
                .all(|c| c as u32 > 32 && (c as u32) < 127),
        "Invalid characters in username"
    );
    if !options.properties.boolean("online-mode") {
        return Ok(login::GameProfile {
            id: Uuid::offline_player(&hello.name),
            name: hello.name,
            properties: Vec::new(),
        });
    }
    let mut challenge = [0; 4];
    rand::thread_rng().fill_bytes(&mut challenge);
    io.send(&login::ClientboundPacket::Hello(login::ClientboundHello {
        server_id: String::new(),
        public_key: options.keys.public_key_der().to_vec(),
        challenge: challenge.to_vec(),
        should_authenticate: true,
    }))
    .await?;
    let login::ServerboundPacket::Key(key) =
        login::ServerboundPacket::from_bytes(&io.read().await?)?
    else {
        bail!("expected encryption response");
    };
    ensure!(
        options.keys.decrypt(&key.encrypted_challenge)? == challenge,
        "Protocol error"
    );
    let secret = options.keys.decrypt_secret(&key.key_bytes)?;
    let hash = server_id_hash("", &secret, options.keys.public_key_der());
    io.encryption(&secret);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(5))
        .build()?;
    let mut request = client
        .get("https://sessionserver.mojang.com/session/minecraft/hasJoined")
        .query(&[
            ("username", hello.name.as_str()),
            ("serverId", hash.as_str()),
        ]);
    if options.properties.boolean("prevent-proxy-connections") {
        request = request.query(&[("ip", address.ip().to_string())]);
    }
    let response = request
        .send()
        .await
        .context("multiplayer.disconnect.authservers_down")?;
    ensure!(
        response.status() != reqwest::StatusCode::NO_CONTENT,
        "multiplayer.disconnect.unverified_username"
    );
    #[derive(Deserialize)]
    struct Property {
        name: String,
        value: String,
        signature: Option<String>,
    }
    #[derive(Deserialize)]
    struct Profile {
        id: String,
        name: String,
        #[serde(default)]
        properties: Vec<Property>,
    }
    let profile: Profile = response.error_for_status()?.json().await?;
    Ok(login::GameProfile {
        id: session_profile_uuid(&profile.id)?,
        name: profile.name,
        properties: profile
            .properties
            .into_iter()
            .map(|p| ProfileProperty {
                name: p.name,
                value: p.value,
                signature: p.signature,
            })
            .collect(),
    })
}
/// Port of LegacyQueryHandler's pre-1.3 and 1.4/1.6 UTF-16 response forms.
async fn legacy_ping(
    socket: &mut TcpStream,
    status: &status::ServerStatus,
    bytes: &[u8],
) -> Result<()> {
    let players = status.players.as_ref();
    let online = players.map_or(0, |p| p.online);
    let max = players.map_or(20, |p| p.max);
    let motd = match &status.description.contents {
        protocol::chat::Contents::Text(s) => s.as_str(),
        _ => "A Minecraft Server",
    };
    let text = if bytes.len() == 1 {
        format!("{motd}§{online}§{max}")
    } else {
        format!("§1\0{}\0{}\0{motd}\0{online}\0{max}", 127, "26.3")
    };
    let units: Vec<_> = text.encode_utf16().collect();
    let mut response = Vec::with_capacity(units.len() * 2 + 3);
    response.push(0xff);
    response.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for unit in units {
        response.extend_from_slice(&unit.to_be_bytes());
    }
    socket.write_all(&response).await?;
    socket.shutdown().await?;
    Ok(())
}

fn session_profile_uuid(text: &str) -> Result<Uuid> {
    ensure!(
        text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid session profile UUID"
    );
    let number = u128::from_str_radix(text, 16)?;
    Ok(Uuid::from_byte_array(number.to_be_bytes()))
}

fn decode_play(
    bytes: &[u8],
    registries: &crate::registries::RegistrySet,
) -> Result<play::serverbound::Packet> {
    let mut reader = protocol::Reader::with_context(bytes, registries);
    let packet = play::serverbound::Packet::decode(&mut reader)?;
    reader.finish()?;
    Ok(packet)
}
