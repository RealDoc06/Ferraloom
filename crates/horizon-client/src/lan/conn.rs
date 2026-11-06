//! One accepted connection: handshake, status ping, login (offline, or online
//! with encryption and the session service's verdict) and the
//! configuration phase (ServerStatusPacketListenerImpl,
//! ServerLoginPacketListenerImpl, ServerConfigurationPacketListenerImpl). A
//! player that completes configuration continues in the play state: the
//! reader forwards decoded packets to the game thread while a writer frames,
//! compresses and encrypts what the game queues, and answers the keep-alive
//! schedule on its own so a long frame never times a client out. On Linux a
//! socket's reader and writer are a few shared event-driven threads
//! (`io_pool`); elsewhere, and for an in-memory connection, each has a
//! thread of its own.
use super::{
    Joined, NetEvent, Out, Shared,
    data::{CORE_PACK, SyncData},
};
use horizon_protocol::{
    PROTOCOL_VERSION,
    connection::{ConnReader, ConnWriter, Connection},
    crypto::{offline_uuid, server_hash, uuid_string},
    packets::{
        Empty,
        configuration::{self as cfg, ClientInformation, KnownPack},
        handshake, login, play, status,
    },
    text::translatable,
};
use std::{
    cell::Cell,
    net::TcpStream,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicI32, AtomicI64, AtomicU32, AtomicUsize, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    time::{Duration, Instant},
};

/// ServerCommonPacketListenerImpl.KEEPALIVE_TIME and the disconnect timeout.
const KEEP_ALIVE_EVERY: Duration = Duration::from_secs(15);
pub(super) const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(30);
/// Packets queued for one client beyond this many bytes disconnect it.
const MAX_PENDING: usize = 96 << 20;
/// Vanilla's server-side disconnect during login is a JSON component.
fn login_disconnect(c: &mut Connection, reason: &horizon_world::nbt::Tag) {
    let json = component_json(reason);
    let _ =
        c.send(&login::Clientbound::Disconnect(login::Disconnect { reason_json: json }).encode());
}

/// A component as ComponentSerialization.CODEC writes it with JsonOps (and
/// Gson without HTML escaping): plain text collapses to a string, the
/// content fields come first, then `extra`, then the style.
pub fn component_json(tag: &horizon_world::nbt::Tag) -> String {
    component_value(tag).compact()
}
fn component_value(tag: &horizon_world::nbt::Tag) -> crate::server::json::Json {
    use crate::server::json::Json;
    use horizon_world::nbt::Tag;
    const ORDER: &[&str] = &[
        "type",
        "text",
        "translate",
        "fallback",
        "with",
        "score",
        "selector",
        "separator",
        "keybind",
        "nbt",
        "interpret",
        "block",
        "entity",
        "storage",
        "source",
        "object",
        "sprite",
        "atlas",
        "player",
        "hat",
        "extra",
        "color",
        "shadow_color",
        "bold",
        "italic",
        "underlined",
        "strikethrough",
        "obfuscated",
        "click_event",
        "hover_event",
        "insertion",
        "font",
    ];
    match tag {
        Tag::String(s) => Json::String(s.to_string_lossy()),
        Tag::List(items) => Json::Array(items.iter().map(component_value).collect()),
        Tag::Compound(map) => {
            if map.len() == 1
                && let Some(Tag::String(t)) = map.get(&horizon_world::nbt::JavaString::from("text"))
            {
                return Json::String(t.to_string_lossy());
            }
            let mut keys: Vec<String> = map.keys().map(|k| k.to_string_lossy()).collect();
            keys.sort_by_key(|k| ORDER.iter().position(|o| o == k).unwrap_or(ORDER.len()));
            Json::Object(
                keys.into_iter()
                    .map(|k| {
                        let v = &map[&horizon_world::nbt::JavaString::from(k.as_str())];
                        let value = match v {
                            Tag::Byte(b) => Json::Bool(*b != 0),
                            Tag::Int(i) => Json::Number(i.to_string()),
                            other => component_value(other),
                        };
                        (k, value)
                    })
                    .collect(),
            )
        }
        Tag::Byte(b) => Json::Bool(*b != 0),
        Tag::Int(i) => Json::Number(i.to_string()),
        Tag::Long(i) => Json::Number(i.to_string()),
        _ => Json::Null,
    }
}

/// The inverse of [`component_value`]: a JSON component (a server.properties
/// prompt) as ComponentSerialization.CODEC writes it with NbtOps. An array is
/// its first component with the rest appended (`createFromList`); a list of
/// components mixing plain strings and objects has the strings as `{text}`.
pub fn component_tag(v: &crate::server::json::Json) -> horizon_world::nbt::Tag {
    use crate::server::json::Json;
    use horizon_world::nbt::{JavaString, Tag};
    fn list(items: &[Json]) -> Tag {
        same_kind(items.iter().map(component_tag).collect())
    }
    fn same_kind(mut tags: Vec<Tag>) -> Tag {
        if tags.iter().any(|t| matches!(t, Tag::Compound(_))) {
            for t in &mut tags {
                if let Tag::String(s) = t {
                    *t = Tag::compound([("text", Tag::String(s.clone()))]);
                }
            }
        }
        Tag::List(tags)
    }
    fn value(v: &Json) -> Tag {
        match v {
            Json::Bool(b) => Tag::Byte(i8::from(*b)),
            Json::Number(n) => n.parse().map_or_else(
                |_| Tag::Double(n.parse::<f64>().unwrap_or(0.0).to_bits()),
                Tag::Int,
            ),
            Json::String(s) => Tag::String(JavaString::from(s.as_str())),
            Json::Array(items) => list(items),
            Json::Object(members) => Tag::compound(
                members
                    .iter()
                    .map(|(k, v)| (k.as_str(), value(v)))
                    .collect::<Vec<_>>(),
            ),
            Json::Null => Tag::String(JavaString::from("")),
        }
    }
    match v {
        Json::Array(items) if !items.is_empty() => {
            let mut first = component_tag(&items[0]);
            if let Tag::String(s) = &first {
                first = Tag::compound([("text", Tag::String(s.clone()))]);
            }
            if let Tag::Compound(map) = &mut first {
                let mut extra = match map.remove(&JavaString::from("extra")) {
                    Some(Tag::List(e)) => e,
                    _ => Vec::new(),
                };
                extra.extend(items[1..].iter().map(component_tag));
                map.insert(JavaString::from("extra"), same_kind(extra));
            }
            first
        }
        Json::Object(members) => {
            if let [(key, Json::String(text))] = members.as_slice()
                && key == "text"
            {
                return Tag::String(JavaString::from(text.as_str()));
            }
            Tag::compound(
                members
                    .iter()
                    .map(|(k, v)| {
                        let tag = match k.as_str() {
                            "extra" | "with" => match v {
                                Json::Array(items) => list(items),
                                other => value(other),
                            },
                            _ => value(v),
                        };
                        (k.as_str(), tag)
                    })
                    .collect::<Vec<_>>(),
            )
        }
        Json::String(s) | Json::Number(s) => Tag::String(JavaString::from(s.as_str())),
        Json::Bool(b) => Tag::String(JavaString::from(b.to_string().as_str())),
        _ => Tag::String(JavaString::from("")),
    }
}

/// StringUtil.isValidPlayerName.
pub fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c as u32 > 32 && (c as u32) < 127)
}

thread_local! {
    static HOLDING: Cell<bool> = const { Cell::new(false) };
}

/// While one is alive, the packets this thread sends to each player are held
/// and handed to the player's writer thread together by
/// [`PlayerSender::flush`]: one wake-up per player per tick rather than one
/// per packet. The bytes and their order don't change.
pub struct Holding(bool);
impl Holding {
    pub fn start() -> Self {
        Self(HOLDING.replace(true))
    }
    /// Hold, or not, as `held` says: a pass on the pool's threads sends as
    /// the thread that started it does (`holding`).
    pub fn like(held: bool) -> Self {
        Self(HOLDING.replace(held))
    }
}
/// Whether this thread's packets are held (`Holding`).
pub fn holding() -> bool {
    HOLDING.get()
}
impl Drop for Holding {
    fn drop(&mut self) {
        HOLDING.set(self.0);
    }
}

/// A packet's bytes, made by the first writer thread that gets to them. Every
/// recipient sends the same bytes; the others wait for the first to finish.
pub struct LaterBody(std::sync::LazyLock<Vec<u8>, Box<dyn FnOnce() -> Vec<u8> + Send>>);
impl LaterBody {
    pub fn new(make: Box<dyn FnOnce() -> Vec<u8> + Send>) -> Self {
        Self(std::sync::LazyLock::new(make))
    }
}
impl std::fmt::Debug for LaterBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LaterBody")
    }
}

/// Tells a connection's writer it has something to do: packets queued, or a
/// slot filled. Only a pooled writer (`io_pool`) needs telling; a writer
/// thread waits on its channel and its slots.
pub(super) type Wake = Arc<dyn Fn() + Send + Sync>;

/// A place in a connection's queue for packets a pass beside the tick is still
/// making: what the tick queues after it goes out after them, and the writer
/// waits here for them. The owner fills it ([`SlotFiller`]).
pub struct Slot {
    pub(super) bodies: std::sync::OnceLock<Vec<PacketBody>>,
    pending: Arc<AtomicUsize>,
    wake: Option<Wake>,
}
impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Slot")
    }
}

/// The pass's end of a [`Slot`]. Dropped unfilled (a pass that panicked), it
/// leaves the slot empty rather than the writer thread stuck.
pub struct SlotFiller(Arc<Slot>);
impl SlotFiller {
    /// The packets for this place, which count in the queue from now.
    pub fn fill(&self, bodies: Vec<PacketBody>) {
        let bytes: usize = bodies.iter().map(PacketBody::queued_bytes).sum();
        self.0.pending.fetch_add(bytes, Ordering::Relaxed);
        self.set(bodies);
    }
    fn set(&self, bodies: Vec<PacketBody>) {
        if self.0.bodies.set(bodies).is_ok()
            && let Some(wake) = &self.0.wake
        {
            wake();
        }
    }
}
impl Drop for SlotFiller {
    fn drop(&mut self) {
        self.set(Vec::new());
    }
}

/// A private packet buffer or immutable bytes shared by several recipients.
#[derive(Clone, Debug)]
pub enum PacketBody {
    Owned(Vec<u8>),
    Shared(Arc<[u8]>),
    /// Several packets end to end, each ending at one of `ends`: a player's
    /// many small ones of a tick for one allocation. Sent one by one.
    Run {
        bytes: Vec<u8>,
        ends: Vec<u32>,
    },
    /// One packet the writer thread makes when it comes to it.
    Later(Arc<LaterBody>),
    /// Packets that come from a pass beside the tick, in their place. Has no
    /// bytes of its own to look at: see [`Self::wait`].
    Slot(Arc<Slot>),
}

impl PacketBody {
    /// What it counts for in the connection's queue until it is written.
    pub(super) fn queued_bytes(&self) -> usize {
        match self {
            Self::Later(_) => super::chunks::LATER_BODY_BYTES,
            // Counted when it is filled.
            Self::Slot(_) => 0,
            known => known.len(),
        }
    }
    /// A slot not filled yet: [`Self::wait`] would block.
    fn is_waiting(&self) -> bool {
        matches!(self, Self::Slot(slot) if slot.bodies.get().is_none())
    }
    /// The bodies this stands for, in order: itself, or what its slot is
    /// filled with, once it is.
    pub fn wait(&self) -> &[PacketBody] {
        match self {
            Self::Slot(slot) => slot.bodies.wait(),
            one => std::slice::from_ref(one),
        }
    }
    /// The packets of the body, in order.
    pub fn packets(&self) -> impl Iterator<Item = &[u8]> {
        let (bytes, ends): (&[u8], &[u32]) = match self {
            Self::Run { bytes, ends } => (bytes, ends),
            one => (one, &[]),
        };
        let single = (!matches!(self, Self::Run { .. })).then_some(bytes);
        let mut start = 0;
        single.into_iter().chain(ends.iter().map(move |&end| {
            let packet = &bytes[start..end as usize];
            start = end as usize;
            packet
        }))
    }
}

/// Same kind and same bytes (a slot: the same slot).
#[cfg(test)]
impl PartialEq for PacketBody {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Slot(a), Self::Slot(b)) => Arc::ptr_eq(a, b),
            (Self::Slot(_), _) | (_, Self::Slot(_)) => false,
            _ => {
                std::mem::discriminant(self) == std::mem::discriminant(other)
                    && self.packets().eq(other.packets())
            }
        }
    }
}

impl std::ops::Deref for PacketBody {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(body) => body,
            Self::Shared(body) => body,
            Self::Run { bytes, .. } => bytes,
            Self::Later(body) => &body.0,
            Self::Slot(_) => unreachable!("a slot's bodies are read with wait"),
        }
    }
}

/// The sending half given to the game thread.
#[derive(Clone)]
pub struct PlayerSender {
    pub tx: Sender<Out>,
    pub pending: Arc<AtomicUsize>,
    held: Arc<Mutex<Vec<PacketBody>>>,
    /// What [`flush_all`] gave the post thread and it hasn't passed on yet.
    posted: Arc<AtomicUsize>,
    /// Told after every [`Out`] and slot this sends: a pooled writer's.
    wake: Option<Wake>,
}
impl PlayerSender {
    pub fn new(tx: Sender<Out>, pending: Arc<AtomicUsize>) -> Self {
        Self {
            tx,
            pending,
            held: Arc::default(),
            posted: Arc::default(),
            wake: None,
        }
    }
    fn held(&self) -> std::sync::MutexGuard<'_, Vec<PacketBody>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
    pub fn send(&self, body: Vec<u8>) {
        self.send_body(PacketBody::Owned(body));
    }
    /// `Guest::send` with only the sender at hand.
    pub fn send_packet(&self, packet: &horizon_protocol::packets::play::Clientbound) {
        self.send(packet.encode());
    }
    /// Send bytes other players get too, without a copy for each.
    pub fn send_shared(&self, body: &Arc<[u8]>) {
        self.send_body(PacketBody::Shared(body.clone()));
    }
    fn send_body(&self, body: PacketBody) {
        self.pending
            .fetch_add(body.queued_bytes(), Ordering::Relaxed);
        if HOLDING.get() {
            self.held().push(body);
        } else {
            self.flush();
            self.deliver(match body {
                PacketBody::Owned(body) => Out::Packet(body),
                shared => Out::Packets(vec![shared]),
            });
        }
    }
    /// Keep a place in the queue for packets that come later.
    pub fn reserve(&self) -> SlotFiller {
        let slot = Arc::new(Slot {
            bodies: Default::default(),
            pending: self.pending.clone(),
            wake: self.wake.clone(),
        });
        self.send_body(PacketBody::Slot(slot.clone()));
        SlotFiller(slot)
    }
    /// Several packets in order, as [`Self::send`] each: under one lock
    /// while the tick holds them.
    pub fn send_many(&self, bodies: impl IntoIterator<Item = Vec<u8>>) {
        self.send_bodies(bodies.into_iter().map(PacketBody::Owned));
    }
    /// Queue shared and private packet bodies together, retaining their order.
    pub fn send_bodies(&self, bodies: impl IntoIterator<Item = PacketBody>) {
        if !HOLDING.get() {
            bodies.into_iter().for_each(|body| self.send_body(body));
            return;
        }
        let mut held = self.held();
        for body in bodies {
            self.pending
                .fetch_add(body.queued_bytes(), Ordering::Relaxed);
            held.push(body);
        }
    }
    /// Hand the held packets to the writer thread.
    pub fn flush(&self) {
        let held = std::mem::take(&mut *self.held());
        if !held.is_empty() {
            self.deliver(Out::Packets(held));
        }
    }
    /// Send a play disconnect with this reason after what was sent before.
    pub fn disconnect(&self, reason: horizon_world::nbt::Tag) {
        self.flush();
        self.deliver(Out::Disconnect(reason));
    }
    /// To the writer, after anything still with the post thread.
    fn deliver(&self, out: Out) {
        if self.posted.load(Ordering::Acquire) == 0 {
            let _ = self.tx.send(out);
            if let Some(wake) = &self.wake {
                wake();
            }
        } else {
            post(vec![self.posting(out)]);
        }
    }
    fn posting(&self, out: Out) -> Posted {
        self.posted.fetch_add(1, Ordering::AcqRel);
        Posted {
            tx: self.tx.clone(),
            out,
            posted: self.posted.clone(),
            wake: self.wake.clone(),
        }
    }
}

/// Hand every player's held packets to their writer threads. Waking a writer
/// costs a system call, and on a busy machine often the rest of the core's
/// time slice; one thread of its own wakes them, so the game thread wakes one
/// thread per tick, not one per player. Each player's packets keep their
/// order, also with what is sent to it next.
pub fn flush_all<'a>(senders: impl IntoIterator<Item = &'a PlayerSender>) {
    let batch: Vec<Posted> = senders
        .into_iter()
        .filter_map(|s| {
            let held = std::mem::take(&mut *s.held());
            (!held.is_empty()).then(|| s.posting(Out::Packets(held)))
        })
        .collect();
    if !batch.is_empty() {
        post(batch);
    }
}

/// Packets on their way through the post thread.
struct Posted {
    tx: Sender<Out>,
    out: Out,
    posted: Arc<AtomicUsize>,
    wake: Option<Wake>,
}
impl Posted {
    fn hand_over(self) {
        let _ = self.tx.send(self.out);
        if let Some(wake) = &self.wake {
            wake();
        }
        self.posted.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Give a batch to the post thread, started the first time; without one
/// (the thread could not start) the batch is handed over here.
fn post(batch: Vec<Posted>) {
    static POST: std::sync::OnceLock<Option<Sender<Vec<Posted>>>> = std::sync::OnceLock::new();
    let post = POST.get_or_init(|| {
        let (tx, rx) = channel::<Vec<Posted>>();
        std::thread::Builder::new()
            .name("horizon-lan-post".to_owned())
            .spawn(move || {
                horizon_threads::worker_affinity();
                for batch in rx {
                    batch.into_iter().for_each(Posted::hand_over);
                }
            })
            .ok()
            .map(|_| tx)
    });
    let unsent = match post {
        Some(tx) => tx.send(batch).err().map(|e| e.0),
        None => Some(batch),
    };
    unsent.into_iter().flatten().for_each(Posted::hand_over);
}

/// LegacyQueryHandler: the pre-1.7 server list ping. What the first read
/// holds decides the reply (1.3 and older: no more bytes; 1.4 and 1.5: 0x01;
/// 1.6: 0x01 then the MC|PingHost payload); anything else gets no reply.
fn serve_legacy(mut stream: TcpStream, shared: &Shared) -> Result<(), String> {
    use std::io::{Read, Write};
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut data = Vec::new();
    let mut chunk = [0u8; 1024];
    // The bytes of the first read: what arrived together.
    if let Ok(n) = stream.read(&mut chunk) {
        data.extend_from_slice(&chunk[..n]);
    }
    let in_string = |d: &[u8], at: &mut usize| -> Option<String> {
        let len = usize::from(u16::from_be_bytes([*d.get(*at)?, *d.get(*at + 1)?]));
        let bytes = d.get(*at + 2..*at + 2 + len * 2)?;
        *at += 2 + len * 2;
        Some(String::from_utf16_lossy(
            &bytes
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        ))
    };
    let motd = shared.motd();
    let (online, max) = (shared.online(), shared.max_players());
    let body = if data.len() == 1 {
        format!("{motd}\u{a7}{online}\u{a7}{max}")
    } else {
        if data[1] != 1 {
            return Ok(());
        }
        if data.len() > 2 {
            // readCustomPayloadPacket.
            let mut at = 3;
            let valid = data[2] == 0xFA
                && in_string(&data, &mut at).as_deref() == Some("MC|PingHost")
                && data
                    .get(at..at + 2)
                    .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
                    == Some(data.len().saturating_sub(at + 2))
                && {
                    at += 2;
                    data.get(at).is_some_and(|v| *v >= 73)
                }
                && {
                    at += 1;
                    in_string(&data, &mut at).is_some()
                }
                && data
                    .get(at..at + 4)
                    .is_some_and(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) <= 65535);
            if !valid {
                return Ok(());
            }
        }
        format!(
            "\u{a7}1\0127\0{}\0{motd}\0{online}\0{max}",
            horizon_core::VANILLA_VERSION
        )
    };
    let units: Vec<u16> = body.encode_utf16().collect();
    let mut out = vec![0xFF];
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for u in units {
        out.extend_from_slice(&u.to_be_bytes());
    }
    stream.write_all(&out).map_err(|e| e.to_string())?;
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
}

pub fn serve(
    stream: TcpStream,
    address: std::net::SocketAddr,
    shared: Arc<Shared>,
) -> Result<(), String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    // LegacyQueryHandler, ahead of the frame decoder while status is on.
    let mut first = [0u8; 1];
    if shared.config.enable_status
        && stream.peek(&mut first).is_ok_and(|n| n == 1)
        && first[0] == 0xFE
    {
        return serve_legacy(stream, &shared);
    }
    serve_connection(Connection::from_stream(stream).map_err(e)?, address, shared)
}

/// `ServerConnectionListener.startMemoryChannel`: an integrated server's
/// own player, over one end of an in-memory pipe.
pub fn serve_local(
    pipe: horizon_protocol::connection::Pipe,
    shared: Arc<Shared>,
) -> Result<(), String> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    serve_connection(Connection::local(pipe), address, shared)
}

fn serve_connection(
    mut c: Connection,
    address: std::net::SocketAddr,
    shared: Arc<Shared>,
) -> Result<(), String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    c.reader
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(e)?;
    let (id, payload) = c.recv().map_err(e)?;
    let intention = match handshake::Serverbound::decode(id, &payload).map_err(e)? {
        handshake::Serverbound::Intention(i) => i,
        _ => return Err("expected a handshake".into()),
    };
    match intention.intent {
        // ServerHandshakePacketListenerImpl: a server that does not reply
        // to status requests closes the connection.
        handshake::STATUS if !shared.config.enable_status => Ok(()),
        handshake::STATUS => serve_status(&mut c, &shared),
        handshake::TRANSFER if !shared.config.accepts_transfers => {
            login_disconnect(
                &mut c,
                &translatable("multiplayer.disconnect.transfers_disabled", &[]),
            );
            Ok(())
        }
        handshake::LOGIN | handshake::TRANSFER => {
            if intention.protocol != PROTOCOL_VERSION {
                let key = if intention.protocol < 754 {
                    "multiplayer.disconnect.outdated_client"
                } else {
                    "multiplayer.disconnect.incompatible"
                };
                let json = format!(
                    "{{\"translate\":\"{key}\",\"with\":[\"{}\"]}}",
                    horizon_protocol::GAME_VERSION
                );
                let _ = c.send(
                    &login::Clientbound::Disconnect(login::Disconnect { reason_json: json })
                        .encode(),
                );
                return Ok(());
            }
            serve_login(c, address, &shared)
        }
        other => Err(format!("unknown intent {other}")),
    }
}

fn serve_status(c: &mut Connection, shared: &Shared) -> Result<(), String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    let mut requested = false;
    loop {
        let (id, payload) = c.recv().map_err(e)?;
        match status::Serverbound::decode(id, &payload).map_err(e)? {
            // ServerStatusPacketListenerImpl: a second request disconnects.
            status::Serverbound::StatusRequest(_) if requested => return Ok(()),
            status::Serverbound::StatusRequest(_) => {
                requested = true;
                let json = shared.status_json();
                c.send(
                    &status::Clientbound::StatusResponse(status::StatusResponse { json }).encode(),
                )
                .map_err(e)?;
            }
            status::Serverbound::PingRequest(p) => {
                c.send(&status::Clientbound::PongResponse(p).encode())
                    .map_err(e)?;
                return Ok(());
            }
            status::Serverbound::Unknown(..) => return Ok(()),
        }
    }
}

/// ServerLoginPacketListenerImpl.getUserName: the name and the address
/// (Connection.getLoggableAddress, `/ip:port` as InetSocketAddress prints it).
fn user_name(name: &str, address: std::net::SocketAddr, log: &super::NetLog) -> String {
    if log.log_ips {
        format!("{name} (/{address})")
    } else {
        format!("{name} (<ip address withheld>)")
    }
}

/// ServerLoginPacketListenerImpl.disconnect: log the reason's text and send
/// it. Returns the text, for the "lost connection" line that follows.
fn refuse(
    c: &mut Connection,
    shared: &Shared,
    thread: &str,
    user: &str,
    reason: &horizon_world::nbt::Tag,
) -> String {
    let text = shared
        .config
        .log
        .as_ref()
        .map(|log| horizon_protocol::text::plain(reason, &log.lang))
        .unwrap_or_default();
    if let Some(log) = &shared.config.log {
        (log.write)(thread, "INFO", &format!("Disconnecting {user}: {text}"));
    }
    login_disconnect(c, reason);
    text
}

/// ServerLoginPacketListenerImpl.onDisconnect, once the server thread sees
/// the connection closed: by a refusal, or by the client ("Disconnected").
fn lost(shared: &Shared, user: &str, text: &str) {
    if let Some(log) = &shared.config.log {
        (log.write)(
            "Server thread",
            "INFO",
            &format!("{user} lost connection: {text}"),
        );
    }
}

/// UNIQUE_THREAD_ID: the User Authenticator threads are numbered from 1.
static AUTHENTICATORS: AtomicU32 = AtomicU32::new(0);

/// ServerLoginPacketListenerImpl.handleKey and the User Authenticator
/// thread: check the client's answer to the encryption request, encrypt the
/// connection, and ask the session service whether the player joined with
/// this server's hash. The profile, or the text of the refusal.
fn authenticate(
    c: &mut Connection,
    shared: &Shared,
    online: &super::OnlineMode,
    name: &str,
    address: std::net::SocketAddr,
    user: &str,
) -> Result<Result<(u128, Vec<login::Property>), String>, String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    let mut challenge = [0u8; 4];
    horizon_protocol::crypto::random_bytes(&mut challenge).map_err(e)?;
    c.send(
        &login::Clientbound::Hello(login::Hello {
            server_id: String::new(),
            public_key: online.public_der.clone(),
            challenge: challenge.to_vec(),
            should_authenticate: true,
        })
        .encode(),
    )
    .map_err(e)?;
    let (id, payload) = c.recv().map_err(e)?;
    let key = match login::Serverbound::decode(id, &payload).map_err(e)? {
        login::Serverbound::Key(k) => k,
        _ => return Err("Unexpected packet during login".into()),
    };
    // ServerboundKeyPacket.isChallengeValid and getSecretKey.
    let protocol_error = || "Protocol error".to_owned();
    if online.key.decrypt(&key.challenge).ok().as_deref() != Some(&challenge[..]) {
        return Err(protocol_error());
    }
    let secret: [u8; 16] = online
        .key
        .decrypt(&key.key)
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or_else(protocol_error)?;
    let digest = server_hash("", &secret, &online.public_der);
    c.enable_encryption(&secret);
    let thread = format!(
        "User Authenticator #{}",
        AUTHENTICATORS.fetch_add(1, Ordering::Relaxed) + 1
    );
    let log = |level: &str, line: &str| {
        if let Some(log) = &shared.config.log {
            (log.write)(&thread, level, line);
        }
    };
    let ip = online.prevent_proxy.then(|| address.ip());
    match crate::auth::has_joined(name, &digest, ip) {
        Ok(Some(profile)) => {
            log(
                "INFO",
                &format!("UUID of player {name} is {}", uuid_string(profile.uuid)),
            );
            Ok(Ok((profile.uuid, profile.properties)))
        }
        Ok(None) => {
            let text = refuse(
                c,
                shared,
                &thread,
                user,
                &translatable("multiplayer.disconnect.unverified_username", &[]),
            );
            log(
                "ERROR",
                &format!("Username '{name}' tried to join with an invalid session"),
            );
            Ok(Err(text))
        }
        Err(_) => {
            let text = refuse(
                c,
                shared,
                &thread,
                user,
                &translatable("multiplayer.disconnect.authservers_down", &[]),
            );
            log(
                "ERROR",
                "Couldn't verify username because servers are unavailable",
            );
            Ok(Err(text))
        }
    }
}

fn serve_login(
    mut c: Connection,
    address: std::net::SocketAddr,
    shared: &Arc<Shared>,
) -> Result<(), String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    let (id, payload) = c.recv().map_err(e)?;
    let hello = match login::Serverbound::decode(id, &payload).map_err(e)? {
        login::Serverbound::Hello(h) => h,
        _ => return Err("expected a login hello".into()),
    };
    if !valid_name(&hello.name) {
        login_disconnect(
            &mut c,
            &translatable("multiplayer.disconnect.invalid_name", &[]),
        );
        return Ok(());
    }
    let user = shared
        .config
        .log
        .as_ref()
        .map(|log| user_name(&hello.name, address, log))
        .unwrap_or_default();
    // handleHello: online mode authenticates first; offline mode derives the
    // profile from the name.
    let (uuid, properties) = match &shared.config.online {
        Some(online) => match authenticate(&mut c, shared, online, &hello.name, address, &user) {
            Ok(Ok(profile)) => profile,
            Ok(Err(refused)) => {
                lost(shared, &user, &refused);
                return Ok(());
            }
            Err(err) => {
                lost(shared, &user, "Disconnected");
                return Err(err);
            }
        },
        None => (offline_uuid(&hello.name), Vec::new()),
    };
    // verifyLoginAndFinishConnectionSetup: PlayerList.canPlayerLogin, then
    // duplicates and a full server.
    if let Some(reason) = shared.try_admit(address, uuid, &hello.name) {
        let text = refuse(&mut c, shared, "Server thread", &user, &reason);
        lost(shared, &user, &text);
        return Ok(());
    }
    let result = login_and_configure(&mut c, shared, uuid, &hello.name, properties);
    match result {
        Ok(Some(parts)) => {
            play(c, shared, parts, address);
            Ok(())
        }
        Ok(None) => {
            shared.release(uuid);
            Ok(())
        }
        Err(err) => {
            shared.release(uuid);
            // ServerConfigurationPacketListenerImpl.onDisconnect.
            let profile = format!("{} ({})", hello.name, uuid_string(uuid));
            lost(shared, &profile, "Disconnected");
            Err(err)
        }
    }
}

struct Parts {
    uuid: u128,
    name: String,
    properties: Vec<login::Property>,
    info: ClientInformation,
    brand: String,
}

fn login_and_configure(
    c: &mut Connection,
    shared: &Arc<Shared>,
    uuid: u128,
    name: &str,
    properties: Vec<login::Property>,
) -> Result<Option<Parts>, String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    // ServerLoginPacketListenerImpl: memory connections are not compressed.
    let threshold = shared.config.compression_threshold;
    if threshold >= 0 && !c.is_memory() {
        c.send(&login::Clientbound::Compression(login::Compression { threshold }).encode())
            .map_err(e)?;
        c.set_compression(threshold);
    }
    c.send(
        &login::Clientbound::LoginFinished(login::LoginFinished {
            profile: login::GameProfile {
                uuid,
                name: name.to_owned(),
                properties: properties.clone(),
            },
            session: uuid,
        })
        .encode(),
    )
    .map_err(e)?;
    // The client may send its settings before acknowledging the login.
    loop {
        let (id, payload) = c.recv().map_err(e)?;
        match login::Serverbound::decode(id, &payload).map_err(e)? {
            login::Serverbound::LoginAcknowledged(_) => break,
            login::Serverbound::CustomQueryAnswer(_) | login::Serverbound::CookieResponse(_) => {}
            _ => return Err("unexpected packet during login".into()),
        }
    }
    // Configuration.
    let sync: &SyncData = &shared.sync;
    let send = |c: &mut Connection, p: cfg::Clientbound| c.send(&p.encode()).map_err(e);
    send(
        c,
        cfg::Clientbound::CustomPayload(cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: {
                let mut w = horizon_protocol::buf::Writer::new();
                // MinecraftServer.getServerModName.
                w.string("vanilla");
                w.data
            },
        }),
    )?;
    // ServerConfigurationPacketListenerImpl.startConfiguration: links, only
    // when there are any (MinecraftServer.serverLinks is empty elsewhere).
    if !shared.config.server_links.is_empty() {
        send(
            c,
            cfg::Clientbound::ServerLinks(cfg::ServerLinks {
                links: shared.config.server_links.clone(),
            }),
        )?;
    }
    send(
        c,
        cfg::Clientbound::UpdateEnabledFeatures(cfg::UpdateEnabledFeatures {
            features: sync.features.clone(),
        }),
    )?;
    let core = KnownPack {
        namespace: CORE_PACK.0.into(),
        id: CORE_PACK.1.into(),
        version: CORE_PACK.2.into(),
    };
    send(
        c,
        cfg::Clientbound::SelectKnownPacks(cfg::SelectKnownPacks {
            packs: vec![core.clone()],
        }),
    )?;
    let mut info = ClientInformation::new(10);
    let mut brand = String::from("vanilla");
    let known_pack = loop {
        let (id, payload) = c.recv().map_err(e)?;
        match cfg::Serverbound::decode(id, &payload).map_err(e)? {
            cfg::Serverbound::ClientInformation(i) => info = i,
            cfg::Serverbound::CustomPayload(p) if p.channel == "minecraft:brand" => {
                brand = horizon_protocol::buf::Reader::new(&p.data)
                    .string(32767)
                    .unwrap_or(brand);
            }
            cfg::Serverbound::SelectKnownPacks(p) => break p.packs == vec![core.clone()],
            _ => {}
        }
    };
    // A Horizon client hears that this server runs its mods' quiet commands
    // (horizon_protocol::quiet); a vanilla one is told nothing.
    if brand == "horizon" {
        send(
            c,
            cfg::Clientbound::CustomPayload(cfg::CustomPayload {
                channel: horizon_protocol::quiet::CHANNEL.into(),
                data: Vec::new(),
            }),
        )?;
    }
    for packet in sync.registry_packets(known_pack) {
        send(c, cfg::Clientbound::RegistryData(packet))?;
    }
    send(c, cfg::Clientbound::UpdateTags(sync.tags.clone()))?;
    if let Some(pack) = &shared.config.resource_pack
        && let Some(rejected) = offer_pack(c, pack, &mut info)?
    {
        let (reason, text) = pack_rejected(shared, name, rejected);
        let _ = send(c, cfg::Clientbound::Disconnect(cfg::Disconnect { reason }));
        lost(shared, &format!("{name} ({})", uuid_string(uuid)), &text);
        return Ok(None);
    }
    send(c, cfg::Clientbound::FinishConfiguration(Empty))?;
    loop {
        let (id, payload) = c.recv().map_err(e)?;
        match cfg::Serverbound::decode(id, &payload).map_err(e)? {
            cfg::Serverbound::FinishConfiguration(_) => break,
            cfg::Serverbound::ClientInformation(i) => info = i,
            _ => {}
        }
    }
    Ok(Some(Parts {
        uuid,
        name: name.to_owned(),
        properties,
        info,
        brand,
    }))
}

/// ServerboundResourcePackPacket.Action.DECLINED.
const PACK_DECLINED: i32 = 1;

/// ServerCommonPacketListenerImpl.handleResourcePackResponse: a player who
/// declined the required pack is disconnected. The reason and its text.
fn pack_rejected(shared: &Shared, name: &str, pack: u128) -> (horizon_world::nbt::Tag, String) {
    let reason = translatable("multiplayer.requiredTexturePrompt.disconnect", &[]);
    let Some(log) = &shared.config.log else {
        return (reason, String::new());
    };
    let line = format!(
        "Disconnecting {name} due to resource pack {} rejection",
        uuid_string(pack)
    );
    (log.write)("Server thread", "INFO", &line);
    let text = horizon_protocol::text::plain(&reason, &log.lang);
    (reason, text)
}

/// ServerResourcePackConfigurationTask: offer the server's pack, then wait
/// for the client's final answer (`Action.isTerminal`: anything but
/// ACCEPTED or DOWNLOADED). The player may take a while to answer the
/// prompt, so the connection is kept alive meanwhile, as
/// ServerConfigurationPacketListenerImpl.tick does. The id of a required
/// pack the client declined, which disconnects it.
fn offer_pack(
    c: &mut Connection,
    pack: &crate::server::properties::ResourcePackInfo,
    info: &mut ClientInformation,
) -> Result<Option<u128>, String> {
    let e = |e: horizon_protocol::Error| e.to_string();
    let push = cfg::ResourcePackPush {
        id: pack.id,
        url: pack.url.clone(),
        hash: pack.hash.clone(),
        required: pack.required,
        prompt: pack.prompt.as_ref().map(component_tag),
    };
    c.send(&cfg::Clientbound::ResourcePackPush(push).encode())
        .map_err(e)?;
    std::thread::scope(|scope| {
        let (stop, stopped) = channel::<()>();
        let writer = &mut c.writer;
        scope.spawn(move || {
            let start = Instant::now();
            while stopped.recv_timeout(KEEP_ALIVE_EVERY) == Err(RecvTimeoutError::Timeout) {
                let id = i64::try_from(start.elapsed().as_millis()).unwrap_or(0);
                let keep_alive = cfg::Clientbound::KeepAlive(cfg::KeepAlive { id });
                if writer.send(&keep_alive.encode()).is_err() {
                    break;
                }
            }
        });
        let answer = loop {
            let (id, payload) = match c.reader.recv() {
                Ok(packet) => packet,
                Err(err) => break Err(err.to_string()),
            };
            match cfg::Serverbound::decode(id, &payload) {
                Ok(cfg::Serverbound::ResourcePack(r)) if !matches!(r.action, 3 | 4) => {
                    let rejected = r.action == PACK_DECLINED && pack.required;
                    break Ok(rejected.then_some(r.id));
                }
                Ok(cfg::Serverbound::ClientInformation(i)) => *info = i,
                Ok(_) => {}
                Err(err) => break Err(err.to_string()),
            }
        };
        drop(stop);
        answer
    })
}

fn play(c: Connection, shared: &Arc<Shared>, parts: Parts, address: std::net::SocketAddr) {
    let conn_id = shared.next_id();
    let (tx, rx) = channel();
    let pending = Arc::new(AtomicUsize::new(0));
    let latency = Arc::new(AtomicI32::new(0));
    #[cfg_attr(not(target_os = "linux"), expect(unused_mut))]
    let mut sender = PlayerSender::new(tx.clone(), pending.clone());
    #[cfg(target_os = "linux")]
    let seat = (!c.is_memory()).then(super::io_pool::seat).flatten();
    #[cfg(target_os = "linux")]
    if let Some(seat) = &seat {
        sender.wake = Some(seat.wake());
    }
    let joined = Joined {
        conn: conn_id,
        uuid: parts.uuid,
        name: parts.name.clone(),
        properties: parts.properties,
        info: parts.info,
        brand: parts.brand,
        sender,
        latency: latency.clone(),
        address,
    };
    let player = Player {
        conn: conn_id,
        uuid: parts.uuid,
        name: parts.name,
        shared: shared.clone(),
        events: shared.events.clone(),
        tx,
        alive: Arc::new(AtomicI64::new(-1)),
        latency,
        pending,
        required_pack: shared
            .config
            .resource_pack
            .as_ref()
            .is_some_and(|p| p.required),
    };
    #[cfg(target_os = "linux")]
    if let Some(seat) = seat {
        seat.serve(c, rx, player, joined);
        return;
    }
    let _ = c.reader.set_read_timeout(Some(KEEP_ALIVE_TIMEOUT));
    let (mut reader, writer) = c.split();
    let writer_thread = {
        let (alive, latency, pending) = (
            player.alive.clone(),
            player.latency.clone(),
            player.pending.clone(),
        );
        std::thread::Builder::new()
            .name(format!("horizon-lan-out-{conn_id}"))
            .spawn(move || {
                // Ticks and frames come first (horizon_threads), but a
                // player's packets are not left to idle time.
                horizon_threads::behind_server_io();
                write_loop(writer, rx, alive, latency, pending)
            })
            .ok()
    };
    let ended = if player
        .events
        .send(NetEvent::Joined(Box::new(joined)))
        .is_ok()
    {
        read_loop(&mut reader, &player)
    } else {
        Ok("server closed".to_owned())
    };
    player.reader_ended(ended);
    if let Some(t) = writer_thread {
        let _ = t.join();
    }
    shared.release(player.uuid);
}

/// Why a player's reader stopped: the reason the game hears, or the id of a
/// required resource pack the player declined, which disconnects them.
pub(super) type Ended = Result<String, u128>;

/// A player in the play state, as both its reader and its writer know it.
pub(super) struct Player {
    pub conn: u32,
    pub uuid: u128,
    name: String,
    pub shared: Arc<Shared>,
    pub events: Sender<NetEvent>,
    /// The connection's own queue, for the reader's last words.
    pub tx: Sender<Out>,
    /// The id of the last keep-alive the client answered.
    pub alive: Arc<AtomicI64>,
    pub latency: Arc<AtomicI32>,
    pub pending: Arc<AtomicUsize>,
    required_pack: bool,
}
impl Player {
    /// Take one packet the client sent: keep-alive answers here, the rest to
    /// the game. Why the connection ends, when this one ends it.
    pub fn receive(&self, id: i32, payload: &[u8]) -> Result<(), Ended> {
        match play::Serverbound::decode(id, payload) {
            Ok(play::Serverbound::KeepAlive(k)) => self.alive.store(k.id, Ordering::Relaxed),
            Ok(play::Serverbound::ResourcePack(r))
                if r.action == PACK_DECLINED && self.required_pack =>
            {
                return Err(Err(r.id));
            }
            Ok(p) => {
                if self
                    .events
                    .send(NetEvent::Packet(self.conn, Box::new(p)))
                    .is_err()
                {
                    return Err(Ok("server closed".to_owned()));
                }
            }
            // A malformed packet disconnects like vanilla's decoder exception.
            Err(e) => return Err(Ok(format!("Invalid packet {id}: {e}"))),
        }
        Ok(())
    }
    /// The reader stopped: the writer sends what is left (a declined pack's
    /// disconnect) and closes, and the game hears the player left.
    pub fn reader_ended(&self, ended: Ended) {
        let reason = match ended {
            Ok(reason) => reason,
            Err(rejected) => {
                let (reason, text) = pack_rejected(&self.shared, &self.name, rejected);
                let _ = self.tx.send(Out::Disconnect(reason));
                text
            }
        };
        let _ = self.tx.send(Out::Close);
        let _ = self.events.send(NetEvent::Left(self.conn, reason));
    }
}

/// Why a failed read ends the connection. The client closing it is
/// Connection.channelInactive without details: multiplayer.disconnect.generic.
pub(super) fn read_failed(e: horizon_protocol::Error) -> String {
    if e.0 == "Connection closed by the server"
        || e.0.contains("reset by peer")
        || e.0.contains("aborted")
    {
        "Disconnected".to_owned()
    } else {
        e.to_string()
    }
}

/// Forward the player's packets until the connection ends.
fn read_loop(reader: &mut ConnReader, player: &Player) -> Ended {
    loop {
        let (id, payload) = match reader.recv() {
            Ok(p) => p,
            Err(e) => return Ok(read_failed(e)),
        };
        if let Err(ended) = player.receive(id, &payload) {
            return ended;
        }
    }
}

/// Packet bodies at least this large are compressed once for every
/// connection: chunk columns, mostly, which every player near them receives.
const SHARED_FRAME_MIN: usize = 8 << 10;
/// The compressed frames kept, in bytes; the oldest go first.
const SHARED_FRAME_BYTES: usize = 32 << 20;

/// Compressed frame contents by the body they were made from: its length,
/// two independently keyed 64-bit hashes of it, and the threshold.
struct SharedFrames {
    hashers: (std::hash::RandomState, std::hash::RandomState),
    frames: Mutex<FrameCache>,
    /// [`SHARED_FRAME_BYTES`].
    limit: usize,
}
type FrameKey = (usize, u64, u64, usize);
#[derive(Default)]
struct FrameCache {
    frames: crate::fast_hash::FxHashMap<FrameKey, Arc<[u8]>>,
    order: std::collections::VecDeque<FrameKey>,
    bytes: usize,
}
impl SharedFrames {
    fn get() -> &'static Self {
        static FRAMES: std::sync::OnceLock<SharedFrames> = std::sync::OnceLock::new();
        FRAMES.get_or_init(|| Self::new(SHARED_FRAME_BYTES))
    }
    fn new(limit: usize) -> Self {
        Self {
            hashers: Default::default(),
            frames: Mutex::default(),
            limit,
        }
    }
    /// `frame::contents(body, Some(threshold))`, compressed only when no
    /// connection has sent the same body lately.
    fn contents(&self, body: &[u8], threshold: usize) -> horizon_protocol::Result<Arc<[u8]>> {
        use std::hash::BuildHasher;
        let key = (
            body.len(),
            self.hashers.0.hash_one(body),
            self.hashers.1.hash_one(body),
            threshold,
        );
        let cache = || self.frames.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(contents) = cache().frames.get(&key) {
            return Ok(contents.clone());
        }
        let contents: Arc<[u8]> = horizon_protocol::frame::contents(body, Some(threshold))?.into();
        let mut cache = cache();
        if cache.frames.insert(key, contents.clone()).is_none() {
            cache.order.push_back(key);
            cache.bytes += contents.len();
            while cache.bytes > self.limit
                && let Some(old) = cache.order.pop_front()
            {
                if let Some(c) = cache.frames.remove(&old) {
                    cache.bytes -= c.len();
                }
            }
        }
        Ok(contents)
    }
}

/// Send one packet body: a large one compressed through [`SharedFrames`],
/// so a chunk column sent to fifty players is compressed once, not fifty
/// times. The bytes on the wire are the same.
fn send_body(writer: &mut ConnWriter, body: &[u8]) -> horizon_protocol::Result<()> {
    match writer.threshold {
        Some(t) if body.len() >= t.max(SHARED_FRAME_MIN) => {
            writer.send_contents(&SharedFrames::get().contents(body, t)?)
        }
        _ => writer.send(body),
    }
}

/// Frame one body's packets after what the writer holds. False when the
/// connection is to close: a packet failed, or the client is too far behind.
pub(super) fn frame_body(
    writer: &mut ConnWriter,
    pending: &AtomicUsize,
    body: &PacketBody,
) -> bool {
    // A later body counted for a guess until now: its bytes from here.
    if matches!(body, PacketBody::Later(_)) {
        pending.fetch_add(body.len(), Ordering::Relaxed);
        pending.fetch_sub(body.queued_bytes(), Ordering::Relaxed);
    }
    body.packets().all(|packet| {
        pending.fetch_sub(packet.len(), Ordering::Relaxed);
        send_body(writer, packet).is_ok() && pending.load(Ordering::Relaxed) <= MAX_PENDING
    })
}

/// Write what the game queued together, for one system call rather than one
/// per packet: the stream's bytes are the same. False when the connection is
/// to close: it failed, or the client is too far behind.
fn write_bodies(
    writer: &mut ConnWriter,
    pending: &AtomicUsize,
    bodies: impl IntoIterator<Item = PacketBody>,
) -> bool {
    writer.start_batch();
    let written = bodies.into_iter().all(|body| {
        // What comes before a slot still being filled goes out meanwhile:
        // the client acknowledges a chunk batch as soon as it has it.
        if body.is_waiting() && writer.finish_batch().is_err() {
            return false;
        }
        writer.start_batch();
        body.wait()
            .iter()
            .all(|body| frame_body(writer, pending, body))
    });
    writer.finish_batch().is_ok() && written
}

/// ServerCommonPacketListenerImpl's keep-alive, which the writer answers for:
/// one every [`KEEP_ALIVE_EVERY`], answered within [`KEEP_ALIVE_TIMEOUT`],
/// each answer's round trip averaged into the latency.
pub(super) struct KeepAlive {
    start: Instant,
    sent_at: Option<(i64, Instant)>,
    next: Instant,
}
/// What [`KeepAlive::due`] asks the writer to send.
pub(super) enum Due {
    /// A keep-alive.
    Send(Vec<u8>),
    /// The timeout's disconnect, after which the connection closes.
    TimedOut(Vec<u8>),
}
impl KeepAlive {
    pub fn new() -> Self {
        let start = Instant::now();
        Self {
            start,
            sent_at: None,
            next: start + KEEP_ALIVE_EVERY,
        }
    }
    /// Take the client's answer, and what is to be sent now, if anything.
    pub fn due(&mut self, alive: &AtomicI64, latency: &AtomicI32) -> Option<Due> {
        let now = Instant::now();
        if let Some((id, at)) = self.sent_at {
            if alive.load(Ordering::Relaxed) == id {
                let ms = i32::try_from(at.elapsed().as_millis()).unwrap_or(i32::MAX);
                let old = latency.load(Ordering::Relaxed);
                latency.store((old * 3 + ms) / 4, Ordering::Relaxed);
                self.sent_at = None;
            } else if at.elapsed() > KEEP_ALIVE_TIMEOUT {
                let reason = translatable("disconnect.timeout", &[]);
                let packet = play::Clientbound::Disconnect(cfg::Disconnect { reason });
                return Some(Due::TimedOut(packet.encode()));
            }
        }
        if self.sent_at.is_some() || now < self.next {
            return None;
        }
        let id = i64::try_from(self.start.elapsed().as_millis()).unwrap_or(0);
        self.sent_at = Some((id, now));
        self.next = now + KEEP_ALIVE_EVERY;
        Some(Due::Send(
            play::Clientbound::KeepAlive(cfg::KeepAlive { id }).encode(),
        ))
    }
}

/// A play disconnect with this reason.
pub(super) fn disconnect_packet(reason: horizon_world::nbt::Tag) -> Vec<u8> {
    play::Clientbound::Disconnect(cfg::Disconnect { reason }).encode()
}

/// How long a disconnect has to reach the client before the socket closes.
pub(super) const DISCONNECT_LINGER: Duration = Duration::from_millis(50);

fn write_loop(
    mut writer: ConnWriter,
    rx: Receiver<Out>,
    alive: Arc<AtomicI64>,
    latency: Arc<AtomicI32>,
    pending: Arc<AtomicUsize>,
) {
    let mut keep_alive = KeepAlive::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Out::Packet(body)) => {
                if !write_bodies(&mut writer, &pending, [PacketBody::Owned(body)]) {
                    break;
                }
            }
            Ok(Out::Packets(bodies)) => {
                if !write_bodies(&mut writer, &pending, bodies) {
                    break;
                }
            }
            Ok(Out::Disconnect(reason)) => {
                let _ = writer.send(&disconnect_packet(reason));
                std::thread::sleep(DISCONNECT_LINGER);
                break;
            }
            Ok(Out::Close) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        match keep_alive.due(&alive, &latency) {
            Some(Due::Send(packet)) => {
                if writer.send(&packet).is_err() {
                    break;
                }
            }
            Some(Due::TimedOut(packet)) => {
                let _ = writer.send(&packet);
                break;
            }
            None => {}
        }
    }
    writer.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 500-player waypoint send path: individual sends versus one lock per recipient.
    #[test]
    #[ignore]
    fn waypoint_send_benchmark() {
        use std::hint::black_box;
        let (tx, _rx) = channel();
        let senders: Vec<_> = (0..500)
            .map(|_| PlayerSender::new(tx.clone(), Arc::default()))
            .collect();
        let _holding = Holding::start();
        for batched in [false, true] {
            let mut samples = Vec::new();
            for _ in 0..7 {
                let mut elapsed = Duration::ZERO;
                for _ in 0..20 {
                    let start = Instant::now();
                    for sender in &senders {
                        let bodies = (0..200).map(|i| black_box(vec![i; 40]));
                        if batched {
                            sender.send_many(bodies);
                        } else {
                            for body in bodies {
                                sender.send(body);
                            }
                        }
                    }
                    elapsed += start.elapsed();
                    for sender in &senders {
                        sender.held().clear();
                        sender.pending.store(0, Ordering::Relaxed);
                    }
                }
                samples.push(elapsed.as_secs_f64() * 1e3 / 20.0);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "waypoint_send batched={batched} median_ms_per_tick={:.3}",
                samples[3]
            );
        }
    }

    #[test]
    fn shared_and_owned_bodies_keep_order_and_pending_bytes() {
        let (tx, rx) = channel();
        let sender = PlayerSender::new(tx, Arc::default());
        let shared: Arc<[u8]> = vec![2, 3].into();
        let holding = Holding::start();
        sender.send(vec![1]);
        sender.send_bodies([PacketBody::Shared(shared.clone())]);
        sender.send(vec![4]);
        drop(holding);
        sender.flush();
        let Out::Packets(bodies) = rx.recv().unwrap() else {
            panic!("expected one held batch")
        };
        assert_eq!(
            bodies.iter().map(|b| &b[..]).collect::<Vec<_>>(),
            [vec![1], vec![2, 3], vec![4]]
        );
        let PacketBody::Shared(body) = &bodies[1] else {
            panic!("shared buffer was copied")
        };
        assert!(Arc::ptr_eq(body, &shared));
        assert_eq!(sender.pending.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn a_slot_keeps_its_place_whenever_it_is_filled() {
        let (tx, rx) = channel();
        let sender = PlayerSender::new(tx, Arc::default());
        let holding = Holding::start();
        sender.send(vec![1]);
        let filler = sender.reserve();
        let dropped = sender.reserve();
        sender.send(vec![4]);
        drop(holding);
        sender.flush();
        assert_eq!(sender.pending.load(Ordering::Relaxed), 2);
        let Out::Packets(bodies) = rx.recv().unwrap() else {
            panic!("expected one held batch")
        };
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                bodies
                    .iter()
                    .flat_map(|b| b.wait())
                    .flat_map(|b| b.packets())
                    .map(<[u8]>::to_vec)
                    .collect::<Vec<_>>()
            });
            drop(dropped);
            filler.fill(vec![PacketBody::Run {
                bytes: vec![2, 3],
                ends: vec![1, 2],
            }]);
            assert_eq!(writer.join().unwrap(), [vec![1], vec![2], vec![3], vec![4]]);
        });
        assert_eq!(sender.pending.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn a_later_body_is_made_once_keeps_its_place_and_counts_in_the_queue() {
        let (tx, rx) = channel();
        let sender = PlayerSender::new(tx, Arc::default());
        let made = Arc::new(AtomicUsize::new(0));
        let body = Arc::new(LaterBody::new(Box::new({
            let made = made.clone();
            move || {
                made.fetch_add(1, Ordering::Relaxed);
                vec![2, 3]
            }
        })));
        let holding = Holding::start();
        sender.send(vec![1]);
        sender.send_bodies([PacketBody::Later(body.clone()), PacketBody::Later(body)]);
        sender.send(vec![4]);
        drop(holding);
        sender.flush();
        // Queueing it asks nothing of it.
        assert_eq!(made.load(Ordering::Relaxed), 0);
        assert_eq!(
            sender.pending.load(Ordering::Relaxed),
            2 + 2 * crate::lan::chunks::LATER_BODY_BYTES
        );
        let Out::Packets(bodies) = rx.recv().unwrap() else {
            panic!("expected one held batch")
        };
        let packets: Vec<_> = bodies.iter().flat_map(|b| b.packets()).collect();
        assert_eq!(packets, [&[1][..], &[2, 3], &[2, 3], &[4]]);
        assert_eq!(made.load(Ordering::Relaxed), 1);
    }

    /// A pooled writer hears of every batch and slot it can now write.
    #[test]
    fn a_pooled_sender_wakes_its_writer() {
        let (tx, _rx) = channel();
        let mut sender = PlayerSender::new(tx, Arc::default());
        let wakes = Arc::new(AtomicUsize::new(0));
        sender.wake = Some({
            let wakes = wakes.clone();
            Arc::new(move || {
                wakes.fetch_add(1, Ordering::Relaxed);
            })
        });
        let woken = || wakes.load(Ordering::Relaxed);
        sender.send(vec![1]);
        assert_eq!(woken(), 1);
        let holding = Holding::start();
        sender.send(vec![2]);
        let filler = sender.reserve();
        let dropped = sender.reserve();
        assert_eq!(woken(), 1);
        drop(holding);
        // Through the post thread.
        flush_all([&sender]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while woken() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        filler.fill(Vec::new());
        assert_eq!(woken(), 3);
        drop(filler);
        assert_eq!(woken(), 3);
        drop(dropped);
        assert_eq!(woken(), 4);
    }

    #[test]
    fn prompts_become_the_components_vanilla_sends() {
        let tag = |json: &str| {
            let tag = component_tag(&crate::server::json::parse(json, true).unwrap());
            component_json(&tag)
        };
        assert_eq!(tag(r#"{"text":"Hi"}"#), r#""Hi""#);
        assert_eq!(tag(r#""Hi""#), r#""Hi""#);
        assert_eq!(
            tag(r#"["Hi ",{"text":"there","bold":true}]"#),
            r#"{"text":"Hi ","extra":[{"text":"there","bold":true}]}"#
        );
        assert_eq!(
            tag(r#"{"translate":"a","with":["b",{"text":"c","color":"red"}]}"#),
            r#"{"translate":"a","with":["b",{"text":"c","color":"red"}]}"#
        );
    }

    fn unpack(out: Out) -> Vec<Vec<u8>> {
        match out {
            Out::Packet(body) => vec![body],
            Out::Packets(bodies) => bodies.into_iter().map(|body| body.to_vec()).collect(),
            Out::Disconnect(_) => vec![b"disconnect".to_vec()],
            Out::Close => vec![b"close".to_vec()],
        }
    }

    fn bodies(rx: &Receiver<Out>) -> Vec<Vec<Vec<u8>>> {
        rx.try_iter().map(unpack).collect()
    }

    #[test]
    fn packets_sent_after_a_posted_tick_follow_it() {
        let (tx, rx) = channel();
        let (tx2, rx2) = channel();
        let sender = PlayerSender::new(tx, Arc::default());
        let other = PlayerSender::new(tx2, Arc::default());
        for round in 0..50u8 {
            let holding = Holding::start();
            sender.send(vec![round, 1]);
            sender.send(vec![round, 2]);
            other.send(vec![round]);
            drop(holding);
            flush_all([&sender, &other]);
            // Straight after the tick, while the post thread may still hold it.
            sender.send(vec![round, 3]);
            let next = || unpack(rx.recv_timeout(Duration::from_secs(10)).unwrap());
            assert_eq!(next(), [vec![round, 1], vec![round, 2]]);
            assert_eq!(next(), [vec![round, 3]]);
            let got = unpack(rx2.recv_timeout(Duration::from_secs(10)).unwrap());
            assert_eq!(got, [vec![round]]);
        }
        let holding = Holding::start();
        sender.send(vec![9]);
        drop(holding);
        flush_all([&sender]);
        sender.disconnect(horizon_world::nbt::Tag::Byte(0));
        let next = || unpack(rx.recv_timeout(Duration::from_secs(10)).unwrap());
        assert_eq!(next(), [vec![9]]);
        assert_eq!(next(), [b"disconnect".to_vec()]);
        // Nothing sent twice, nothing left with the post thread.
        assert!(rx.try_recv().is_err());
        let deadline = Instant::now() + Duration::from_secs(10);
        while sender.posted.load(Ordering::Acquire) != 0 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn shared_frames_are_the_frames_compressed_here() {
        use horizon_protocol::frame;
        let frames = SharedFrames::new(usize::MAX);
        let a: Vec<u8> = (0..40_000u32).map(|i| (i * 7 % 13) as u8).collect();
        let mut b = a.clone();
        b[20_000] ^= 1;
        for (body, threshold) in [(&a, 256), (&b, 256), (&a, 256), (&a, 64), (&b, 256)] {
            let shared = frames.contents(body, threshold).unwrap();
            assert_eq!(*shared, *frame::contents(body, Some(threshold)).unwrap());
        }
        // One frame per body and threshold.
        assert_eq!(frames.frames.lock().unwrap().frames.len(), 3);
    }

    #[test]
    fn shared_frames_keep_the_newest_within_their_limit() {
        let first = SharedFrames::new(0)
            .contents(&[1; 9000], 256)
            .unwrap()
            .len();
        let frames = SharedFrames::new(first * 2);
        for n in 0..5u8 {
            frames.contents(&[n; 9000], 256).unwrap();
        }
        let cache = frames.frames.lock().unwrap();
        assert!((1..=2).contains(&cache.frames.len()));
        assert_eq!(cache.order.len(), cache.frames.len());
        assert_eq!(
            cache.bytes,
            cache.frames.values().map(|c| c.len()).sum::<usize>()
        );
        assert!(cache.bytes <= first * 2);
    }

    #[test]
    fn held_packets_go_together_and_in_order() {
        let (tx, rx) = channel();
        let sender = PlayerSender::new(tx, Arc::default());
        sender.send(vec![1]);
        assert_eq!(bodies(&rx), [vec![vec![1]]]);
        let holding = Holding::start();
        sender.send(vec![2]);
        sender.send(vec![3]);
        assert!(bodies(&rx).is_empty());
        drop(holding);
        // A packet sent after the tick still follows the held ones.
        sender.send(vec![4]);
        assert_eq!(bodies(&rx), [vec![vec![2], vec![3]], vec![vec![4]]]);
        let holding = Holding::start();
        sender.send(vec![5]);
        sender.disconnect(horizon_world::nbt::Tag::Byte(0));
        drop(holding);
        sender.flush();
        assert_eq!(bodies(&rx), [vec![vec![5]], vec![b"disconnect".to_vec()]]);
        assert_eq!(sender.pending.load(Ordering::Relaxed), 5);
    }
}
