//! The locator bar for remote players: ServerWaypointManager and
//! WaypointTransmitter's connections, for players (the transmitters vanilla has
//! by default: `waypoint_transmit_range` is 0 for every other entity).
//!
//! Vanilla updates a player's connections each time the player's position
//! changes (`Entity.setPosRaw`, then `ServerChunkCache.move`). The hub sees the
//! position once a tick, after the moves it handled, and updates from there:
//! the same packets, since a client sends one move a tick. Hiding (crouching,
//! invisibility, a carved pumpkin or a mob head worn) changes
//! `waypoint_transmit_range` and tracks or untracks the waypoint, as
//! `LivingEntity.onAttributeUpdated` does; joining or leaving a team remakes the
//! connections with the team's colour (`ServerScoreboard.updatePlayerWaypoint`).
use super::conn::PacketBody;
use crate::fast_hash::FxHashMap as HashMap;
use horizon_protocol::packets::play::{
    Clientbound as C, TrackedWaypoint, WaypointId, WaypointTarget, waypoint_operation,
};

/// `Player.createAttributes`: both waypoint ranges.
pub const PLAYER_RANGE: f64 = 6.0e7;
/// `WaypointTransmitter.REALLY_FAR_DISTANCE`.
const REALLY_FAR: f32 = 332.0;
/// `EntityAzimuthConnection.update`: half a degree.
const AZIMUTH_STEP: f32 = 0.008_726_646;

/// Items that hide their wearer (`Waypoint.addHideAttribute` on the head slot).
pub const HIDING_HEADS: [&str; 8] = [
    "minecraft:carved_pumpkin",
    "minecraft:skeleton_skull",
    "minecraft:wither_skeleton_skull",
    "minecraft:player_head",
    "minecraft:zombie_head",
    "minecraft:creeper_head",
    "minecraft:dragon_head",
    "minecraft:piglin_head",
];

/// A team colour as the locator bar draws it (`Waypoint.Icon.cloneAndAssignStyle`:
/// `TeamColor.rgb()`, black lightened so it shows).
pub fn team_rgb(color: &str) -> Option<i32> {
    let rgb = match color {
        "black" => return Some(-13_619_152),
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    };
    Some(rgb)
}

/// Waypoint.Icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub style: String,
    pub color: Option<i32>,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            style: "minecraft:default".into(),
            color: None,
        }
    }
}

/// The icon of a player `/waypoint modify` hasn't touched, shared by them.
pub fn default_icon() -> std::sync::Arc<Icon> {
    static DEFAULT: std::sync::OnceLock<std::sync::Arc<Icon>> = std::sync::OnceLock::new();
    DEFAULT.get_or_init(Default::default).clone()
}

/// A player as the waypoint manager sees them this tick.
#[derive(Clone, Debug)]
pub struct Player {
    pub uuid: u128,
    /// The connection that receives the player's locator bar (None: the host,
    /// whose own game draws it).
    pub conn: Option<u32>,
    pub position: [f64; 3],
    pub spectator: bool,
    /// `waypoint_transmit_range`: 0 while hidden.
    pub transmit_range: f64,
    pub receive_range: f64,
    /// `Entity.firstTick`: the player has not ticked since joining.
    pub first_tick: bool,
    /// The chunk view radius (`ChunkMap.getPlayerViewDistance`).
    pub view_distance: i32,
    /// The player's team and its colour (`TeamColor.rgb`).
    pub team: Option<(String, Option<i32>)>,
    /// The icon `/waypoint modify` set.
    pub icon: std::sync::Arc<Icon>,
}

impl Player {
    fn block(&self) -> [i32; 3] {
        self.position.map(|v| v.floor() as i32)
    }
    #[cfg(test)]
    fn distance_squared(&self, other: &Player) -> f64 {
        (0..3)
            .map(|k| (self.position[k] - other.position[k]).powi(2))
            .sum()
    }
    /// `Entity.distanceTo`.
    #[cfg(test)]
    fn distance_to(&self, other: &Player) -> f32 {
        self.distance_squared(other).sqrt() as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Block([i32; 3]),
    Chunk([i32; 2]),
    Azimuth(f32),
}

#[derive(Clone, Debug)]
struct Connection {
    kind: Kind,
    icon: std::sync::Arc<Icon>,
    /// Last tick (0: none) and receiver view for which this connection was
    /// evaluated. Rows are walked for every pair every tick: keep this small.
    checked: (u32, [i32; 2]),
}

/// What the manager remembers of a player between ticks.
#[derive(Clone)]
struct Memo {
    position: [f64; 3],
    /// The centre of the player's ChunkTrackingView (`ChunkMap.move`).
    view_center: [i32; 2],
    transmitting: bool,
    team: Option<(String, Option<i32>)>,
}

/// Ordered connections retain disconnect packet order; the index serves updates.
#[derive(Default)]
struct Connections {
    ordered: Vec<(usize, Connection)>,
    index: Vec<Option<std::num::NonZeroUsize>>,
}

impl Connections {
    fn put(&mut self, source: usize, connection: Connection) {
        if self.index.len() <= source {
            self.index.resize(source + 1, None);
        }
        if let Some(i) = self.index[source] {
            self.ordered[i.get() - 1].1 = connection;
        } else {
            self.index[source] = std::num::NonZeroUsize::new(self.ordered.len() + 1);
            self.ordered.push((source, connection));
        }
    }

    fn remove(&mut self, source: usize) -> Option<Connection> {
        let i = self.index.get_mut(source)?.take()?.get() - 1;
        let (_, connection) = self.ordered.remove(i);
        for (offset, (id, _)) in self.ordered[i..].iter().enumerate() {
            self.index[*id] = std::num::NonZeroUsize::new(i + offset + 1);
        }
        Some(connection)
    }
}

/// ServerWaypointManager for one level.
#[derive(Default)]
pub struct Waypoints {
    /// The transmitters, in the order they were tracked.
    waypoints: Vec<usize>,
    /// The receivers, in the order they joined.
    players: Vec<usize>,
    /// Connections by receiver, then source.
    connections: Vec<Connections>,
    memo: Vec<Option<Memo>>,
    /// The `locator_bar` game rule as last seen.
    enabled: Option<bool>,
    tick: u64,
    shard: Shard,
    /// This tick's `updateWaypoint` calls, for [`Self::tick_rows`].
    moved: Vec<(usize, usize)>,
    is_waypoint: Vec<bool>,
    /// Each input player's slot this tick.
    input_slots: Vec<usize>,
    /// UUID lookup occurs at the tick boundary; pair traversal uses stable slots.
    slots: HashMap<u128, usize>,
    uuids: Vec<Option<u128>>,
    free: Vec<usize>,
    frame: Vec<SlotFrame>,
    geometry: Vec<Geometry>,
    view: Vec<Option<([i32; 2], i32)>>,
    #[cfg(test)]
    count_only: bool,
    #[cfg(test)]
    emitted: usize,
    #[cfg(test)]
    row_ticks: usize,
}

#[derive(Clone, Copy)]
struct Shard {
    index: usize,
    count: usize,
}

impl Default for Shard {
    fn default() -> Self {
        Self { index: 0, count: 1 }
    }
}

impl Shard {
    fn owner(receiver: u128, count: usize) -> usize {
        use std::hash::BuildHasher;
        let hash = crate::fast_hash::FxBuild::default().hash_one(receiver);
        ((u128::from(hash) * count as u128) >> 64) as usize
    }

    fn owns(self, receiver: u128) -> bool {
        self.count == 1 || Self::owner(receiver, self.count) == self.index
    }
}

/// A local locator update, or all of one receiver's packets of the tick.
pub enum Delivery {
    Local(TrackedWaypoint),
    Encoded(PacketBody),
}

/// Borrow the icon until bytes are written, avoiding a cloned style and the
/// large Clientbound staging vector. Wire layout: TrackedWaypoint::write.
fn write_waypoint(
    data: &mut Vec<u8>,
    source: u128,
    operation: i32,
    icon: &Icon,
    target: WaypointTarget,
) {
    use horizon_protocol::{buf::Writer, ids::play::clientbound::WAYPOINT};
    let mut w = Writer {
        data: std::mem::take(data),
    };
    w.varint(WAYPOINT);
    w.varint(operation);
    w.bool(true);
    w.uuid(source);
    w.string(&icon.style);
    w.bool(icon.color.is_some());
    if let Some(color) = icon.color {
        for shift in [16, 8, 0] {
            w.u8((color >> shift) as u8);
        }
    }
    match target {
        WaypointTarget::Empty => w.varint(0),
        WaypointTarget::Block([x, y, z]) => {
            w.varint(1);
            w.varint(x);
            w.varint(y);
            w.varint(z);
        }
        WaypointTarget::Chunk([x, z]) => {
            w.varint(2);
            w.varint(x);
            w.varint(z);
        }
        WaypointTarget::Azimuth(angle) => {
            w.varint(3);
            w.f32(angle);
        }
    }
    *data = w.data;
}

/// One receiver's packets so far this tick, end to end.
#[derive(Default)]
struct Run {
    conn: u32,
    bytes: Vec<u8>,
    ends: Vec<u32>,
}

/// A shard's packets by receiver: only a receiver's owner writes to it, so
/// each connection's packets are in the serial manager's order without any
/// merge across shards.
#[derive(Default)]
struct Ready {
    runs: Vec<Run>,
    touched: Vec<usize>,
    out: Vec<(u32, Delivery)>,
}

impl Ready {
    fn run(&mut self, receiver: usize, conn: u32) -> &mut Run {
        if self.runs.len() <= receiver {
            self.runs.resize_with(receiver + 1, Run::default);
        }
        let run = &mut self.runs[receiver];
        if run.ends.is_empty() {
            self.touched.push(receiver);
            run.conn = conn;
        }
        run
    }

    /// Each receiver's packets as one body, sized for the next tick's like them.
    fn finish(&mut self) {
        for receiver in self.touched.drain(..) {
            let run = &mut self.runs[receiver];
            let (bytes, ends) = (run.bytes.len(), run.ends.len());
            self.out.push((
                run.conn,
                Delivery::Encoded(PacketBody::Run {
                    bytes: std::mem::replace(&mut run.bytes, Vec::with_capacity(bytes)),
                    ends: std::mem::replace(&mut run.ends, Vec::with_capacity(ends)),
                }),
            ));
        }
    }
}

enum Output<'a> {
    Typed(&'a mut Vec<(u32, C)>),
    Encoded {
        ready: &'a mut Ready,
        local: Option<u32>,
    },
}

#[derive(Default)]
struct ReceiverShard {
    manager: Waypoints,
    ready: Ready,
}

/// Owned only while a pass runs; the original player buffer returns to the caller.
struct Snapshot {
    players: Vec<Player>,
    geometry: Vec<Geometry>,
    away: std::sync::Arc<Vec<(u128, u32)>>,
    respawned: std::sync::Arc<Vec<u128>>,
    enabled: bool,
    local: Option<u32>,
}

struct Work {
    shard: ReceiverShard,
    snapshot: std::sync::Arc<Snapshot>,
}

/// A persistent helper parked on a mailbox between passes. Only its owned
/// receiver state and an immutable snapshot cross the thread boundary.
struct Helper {
    tx: Option<std::sync::mpsc::Sender<Work>>,
    rx: std::sync::mpsc::Receiver<ReceiverShard>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Helper {
    fn new() -> Self {
        let (tx, work) = std::sync::mpsc::channel::<Work>();
        let (done, rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("waypoint-worker".into())
            .spawn(move || {
                horizon_threads::worker_affinity();
                while let Ok(Work {
                    mut shard,
                    snapshot,
                }) = work.recv()
                {
                    shard.tick(&snapshot);
                    // Release the input before publishing completion, so the caller
                    // can recover its original vector without cloning any players.
                    drop(snapshot);
                    if done.send(shard).is_err() {
                        break;
                    }
                }
            })
            .expect("start waypoint helper");
        Self {
            tx: Some(tx),
            rx,
            thread: Some(thread),
        }
    }

    fn send(&self, work: Work) {
        self.tx
            .as_ref()
            .unwrap()
            .send(work)
            .unwrap_or_else(|_| panic!("waypoint helper stopped"));
    }

    fn receive(&self) -> ReceiverShard {
        self.rx.recv().expect("waypoint helper stopped")
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl ReceiverShard {
    fn tick(&mut self, s: &Snapshot) {
        self.manager.tick_level_cached(
            (&s.players, &s.geometry),
            &s.away,
            &s.respawned,
            s.enabled,
            Output::Encoded {
                ready: &mut self.ready,
                local: s.local,
            },
        );
        self.ready.finish();
    }
}

/// Receiver rows of ServerWaypointManager evaluated on the server's helper lane.
/// Each shard replays the same lifecycle and view phases over immutable inputs;
/// only its connection rows mutate. A receiver's row and packets live on one
/// shard, so every connection gets its packets in the serial order.
pub struct ParallelWaypoints {
    geometry: Vec<Geometry>,
    shards: Vec<ReceiverShard>,
    helpers: Vec<Helper>,
    #[cfg(test)]
    threads: Option<usize>,
}

impl Default for ParallelWaypoints {
    fn default() -> Self {
        Self {
            geometry: Vec::new(),
            shards: vec![ReceiverShard::default()],
            helpers: Vec::new(),
            #[cfg(test)]
            threads: None,
        }
    }
}

impl ParallelWaypoints {
    fn split(&mut self, count: usize) {
        let original = &mut self.shards[0].manager;
        let connections = std::mem::take(&mut original.connections);
        let replicas = (1..count)
            .map(|index| {
                let replica = Waypoints {
                    waypoints: original.waypoints.clone(),
                    players: original.players.clone(),
                    memo: original.memo.clone(),
                    slots: original.slots.clone(),
                    uuids: original.uuids.clone(),
                    free: original.free.clone(),
                    frame: original.frame.clone(),
                    view: vec![None; original.uuids.len()],
                    connections: (0..original.uuids.len())
                        .map(|_| Connections::default())
                        .collect(),
                    enabled: original.enabled,
                    tick: original.tick,
                    shard: Shard { index, count },
                    ..Waypoints::default()
                };
                ReceiverShard {
                    manager: replica,
                    ..ReceiverShard::default()
                }
            })
            .collect::<Vec<_>>();
        self.shards.extend(replicas);
        self.shards[0].manager.shard.count = count;
        self.shards[0].manager.connections = (0..connections.len())
            .map(|_| Connections::default())
            .collect();
        for (receiver, row) in connections.into_iter().enumerate() {
            if let Some(uuid) = self.shards[0].manager.uuids[receiver] {
                self.shards[Shard::owner(uuid, count)].manager.connections[receiver] = row;
            }
        }
    }

    fn run_persistent(&mut self, snapshot: &std::sync::Arc<Snapshot>) {
        while self.helpers.len() < self.shards.len() - 1 {
            self.helpers.push(Helper::new());
        }
        for (helper, shard) in self.helpers.iter().zip(&mut self.shards[1..]) {
            helper.send(Work {
                shard: std::mem::take(shard),
                snapshot: snapshot.clone(),
            });
        }
        self.shards[0].tick(snapshot);
        for (helper, shard) in self.helpers.iter().zip(&mut self.shards[1..]) {
            *shard = helper.receive();
        }
    }

    /// One pass over a level's players. `out` gets each connection's packets
    /// in the serial order; different connections come in no particular
    /// order. The local receiver's typed updates are applied by the caller
    /// only after the complete pass.
    pub fn tick_level_into(
        &mut self,
        players: &mut Vec<Player>,
        away: &std::sync::Arc<Vec<(u128, u32)>>,
        respawned: &std::sync::Arc<Vec<u128>>,
        locator_bar: bool,
        local: Option<u32>,
        out: &mut Vec<(u32, Delivery)>,
    ) {
        let threads = horizon_threads::worker_threads()
            .map_or_else(
                || std::thread::available_parallelism().map_or(1, |n| n.get()),
                |helpers| helpers + 1,
            )
            .min(8);
        #[cfg(test)]
        let threads = self.threads.unwrap_or(threads);
        if self.shards.len() == 1 && threads > 1 {
            self.split(threads);
        }
        let mut geometry = std::mem::take(&mut self.geometry);
        geometry.clear();
        geometry.extend(players.iter().map(Geometry::new));
        let snapshot = std::sync::Arc::new(Snapshot {
            geometry,
            players: std::mem::take(players),
            away: away.clone(),
            respawned: respawned.clone(),
            enabled: locator_bar,
            local,
        });
        if self.shards.len() == 1 {
            self.shards[0].tick(&snapshot);
        } else {
            self.run_persistent(&snapshot);
        }
        for shard in &mut self.shards {
            out.append(&mut shard.ready.out);
        }
        let snapshot = std::sync::Arc::try_unwrap(snapshot)
            .ok()
            .expect("waypoint helpers released their snapshot");
        *players = snapshot.players;
        self.geometry = snapshot.geometry;
    }
}

/// Tick geometry shared by every shard; arithmetic matches Player's positions.
#[derive(Clone, Copy)]
struct Geometry {
    position: [f64; 3],
    block: [i32; 3],
    chunk: [i32; 2],
}

impl Geometry {
    fn new(player: &Player) -> Self {
        let block = player.block();
        Self {
            position: player.position,
            block,
            chunk: [block[0] >> 4, block[2] >> 4],
        }
    }
}

#[derive(Clone, Copy, Default)]
struct SlotFrame {
    input: Option<usize>,
    owned: bool,
}

#[derive(Clone, Copy)]
struct PlayerRef<'a> {
    player: &'a Player,
    slot: usize,
    geometry: &'a Geometry,
}

impl std::ops::Deref for PlayerRef<'_> {
    type Target = Player;
    fn deref(&self) -> &Player {
        self.player
    }
}

impl PlayerRef<'_> {
    fn distance_squared(&self, other: &Self) -> f64 {
        (0..3)
            .map(|k| (self.geometry.position[k] - other.geometry.position[k]).powi(2))
            .sum()
    }
    fn distance_to(&self, other: &Self) -> f32 {
        self.distance_squared(other).sqrt() as f32
    }
    fn block(self) -> [i32; 3] {
        self.geometry.block
    }
    fn chunk(self) -> [i32; 2] {
        self.geometry.chunk
    }
}

/// One tick's players, and the packets for their connections.
struct Tick<'a> {
    serial: u64,
    players: &'a [Player],
    geometry: &'a [Geometry],
    frame: Vec<SlotFrame>,
    uuids: Vec<Option<u128>>,
    /// Fallback connections for receivers absent from `players`: a player who
    /// left this level still loses its waypoints (`removePlayer` disconnects them).
    away: &'a [(u128, u32)],
    view: Vec<Option<([i32; 2], i32)>>,
    /// Players a respawn replaced this tick: a new ServerPlayer, on its first tick.
    fresh: &'a [u128],
    enabled: bool,
    out: Output<'a>,
    #[cfg(test)]
    count_only: bool,
    #[cfg(test)]
    emitted: usize,
}

impl<'a> Tick<'a> {
    fn player(&self, slot: &usize) -> Option<PlayerRef<'a>> {
        let f = self.frame[*slot];
        Some(PlayerRef {
            player: &self.players[f.input?],
            slot: *slot,
            geometry: &self.geometry[f.input?],
        })
    }

    fn owns(&self, slot: usize) -> bool {
        self.frame[slot].owned
    }
    fn send(
        &mut self,
        receiver: usize,
        source: usize,
        operation: i32,
        icon: &Icon,
        target: WaypointTarget,
    ) {
        let Some(conn) = self.player(&receiver).and_then(|p| p.conn).or_else(|| {
            self.away
                .iter()
                .find(|(uuid, _)| Some(*uuid) == self.uuids[receiver])
                .map(|&(_, conn)| conn)
        }) else {
            return;
        };
        self.emit(receiver, conn, source, operation, icon, target);
    }
    fn emit(
        &mut self,
        receiver: usize,
        conn: u32,
        source: usize,
        operation: i32,
        icon: &Icon,
        target: WaypointTarget,
    ) {
        #[cfg(test)]
        {
            self.emitted += 1;
            if self.count_only {
                return;
            }
        }
        let source = self.uuids[source].expect("live waypoint slot");
        match &mut self.out {
            Output::Typed(out) => out.push((conn, waypoint(source, operation, icon, target))),
            Output::Encoded { ready, local } => {
                if *local == Some(conn) {
                    let C::TrackedWaypoint(w) = waypoint(source, operation, icon, target) else {
                        unreachable!()
                    };
                    ready.out.push((conn, Delivery::Local(w)));
                } else {
                    let run = ready.run(receiver, conn);
                    write_waypoint(&mut run.bytes, source, operation, icon, target);
                    run.ends.push(run.bytes.len() as u32);
                }
            }
        }
    }
    fn connect(&mut self, receiver: usize, source: usize, c: &Connection) {
        let target = match c.kind {
            Kind::Block(p) => WaypointTarget::Block(p),
            Kind::Chunk(p) => WaypointTarget::Chunk(p),
            Kind::Azimuth(a) => WaypointTarget::Azimuth(a),
        };
        self.send(receiver, source, waypoint_operation::TRACK, &c.icon, target);
    }
    /// `ClientboundTrackedWaypointPacket.removeWaypoint`: an empty waypoint with the null icon.
    fn disconnect(&mut self, receiver: usize, source: usize) {
        self.send(
            receiver,
            source,
            waypoint_operation::UNTRACK,
            &default_icon(),
            WaypointTarget::Empty,
        );
    }
    /// `ChunkTrackingView.isInViewDistance` of the receiver's view.
    fn chunk_visible(&self, [x, z]: [i32; 2], receiver: usize) -> bool {
        let Some(&([cx, cz], r)) = self.view[receiver].as_ref() else {
            return false;
        };
        let dx = i64::from(((x - cx).abs() - 1).max(0));
        let dz = i64::from(((z - cz).abs() - 1).max(0));
        dx * dx + dz * dz < i64::from(r) * i64::from(r)
    }
    /// `WaypointTransmitter.doesSourceIgnoreReceiver`.
    fn ignores(&self, source: &PlayerRef<'_>, receiver: &PlayerRef<'_>) -> bool {
        if receiver.spectator {
            return false;
        }
        if source.spectator {
            return true;
        }
        let range = source.transmit_range.min(receiver.receive_range);
        // Well inside the range, float rounding in Entity.distanceTo cannot
        // reach the boundary. Keep vanilla's sqrt and f32 rounding near it.
        let squared = source.distance_squared(receiver);
        if range > 0.0 && range <= f64::from(f32::MAX) && squared < (range * 0.5).powi(2) {
            return false;
        }
        f64::from(squared.sqrt() as f32) >= range
    }
    fn really_far(source: &PlayerRef<'_>, receiver: &PlayerRef<'_>) -> bool {
        source.distance_to(receiver) > REALLY_FAR
    }
    /// `EntityAzimuthConnection`: the angle of the receiver seen from the source,
    /// `Mth.atan2` of the offset turned clockwise.
    fn azimuth(source: &PlayerRef<'_>, receiver: &PlayerRef<'_>) -> f32 {
        let d = [0, 1, 2].map(|k| receiver.geometry.position[k] - source.geometry.position[k]);
        horizon_core::geometry::trig::atan2(d[0], -d[2]) as f32
    }
    /// The source and the receiver, if both are in the level this tick.
    fn pair(&self, source: usize, receiver: usize) -> Option<(PlayerRef<'a>, PlayerRef<'a>)> {
        Some((self.player(&source)?, self.player(&receiver)?))
    }
    /// `LivingEntity.makeWaypointConnectionWith`.
    fn make(&self, (s, r): (PlayerRef<'_>, PlayerRef<'_>)) -> Option<Connection> {
        if s.first_tick
            || self.fresh.contains(&s.player.uuid)
            || s.slot == r.slot
            || self.ignores(&s, &r)
        {
            return None;
        }
        // Waypoint.Icon.cloneAndAssignStyle: the icon's colour, else the team's.
        let icon = match s.team.as_ref().and_then(|t| t.1) {
            Some(color) if s.icon.color.is_none() => std::sync::Arc::new(Icon {
                style: s.icon.style.clone(),
                color: Some(color),
            }),
            _ => s.icon.clone(),
        };
        let kind = if Self::really_far(&s, &r) {
            Kind::Azimuth(Self::azimuth(&s, &r))
        } else if !self.chunk_visible(s.chunk(), r.slot) {
            Kind::Chunk(s.chunk())
        } else {
            Kind::Block(s.block())
        };
        Some(Connection {
            kind,
            icon,
            checked: (0, [0; 2]),
        })
    }
    /// `Connection.isBroken` for each kind.
    fn broken(&self, c: &Connection, (s, r): (PlayerRef<'_>, PlayerRef<'_>)) -> bool {
        let receiver = r.slot;
        match c.kind {
            Kind::Block(last) => {
                let now = s.block();
                (0..3).map(|k| (now[k] - last[k]).abs()).sum::<i32>() > 1 || self.ignores(&s, &r)
            }
            Kind::Chunk(last) => {
                let now = s.chunk();
                let moved = (now[0] - last[0]).abs().max((now[1] - last[1]).abs());
                moved > 1 || self.ignores(&s, &r) || self.chunk_visible(last, receiver)
            }
            Kind::Azimuth(_) => {
                self.ignores(&s, &r)
                    || self.chunk_visible(s.chunk(), receiver)
                    || !Self::really_far(&s, &r)
            }
        }
    }
    /// `Connection.update`: the packet when the waypoint moved.
    fn update(&mut self, c: &mut Connection, (s, r): (PlayerRef<'_>, PlayerRef<'_>)) {
        let target = match &mut c.kind {
            Kind::Block(last) => {
                let now = s.block();
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Block(now)
            }
            Kind::Chunk(last) => {
                let now = s.chunk();
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Chunk(now)
            }
            Kind::Azimuth(last) => {
                let now = Self::azimuth(&s, &r);
                if (now - *last).abs() <= AZIMUTH_STEP {
                    return;
                }
                *last = now;
                WaypointTarget::Azimuth(now)
            }
        };
        if let Some(conn) = r.conn {
            self.emit(
                r.slot,
                conn,
                s.slot,
                waypoint_operation::UPDATE,
                &c.icon,
                target,
            );
        }
    }
}

fn waypoint(source: u128, operation: i32, icon: &Icon, target: WaypointTarget) -> C {
    C::TrackedWaypoint(TrackedWaypoint {
        operation,
        id: WaypointId::Uuid(source),
        style: icon.style.clone(),
        color: icon.color,
        target,
    })
}

impl Waypoints {
    fn put(&mut self, receiver: usize, source: usize, c: Connection) {
        self.connections[receiver].put(source, c);
    }
    fn take(&mut self, receiver: usize, source: usize) -> Option<Connection> {
        self.connections.get_mut(receiver)?.remove(source)
    }

    /// `createConnection`.
    fn create_connection(&mut self, t: &mut Tick, receiver: usize, source: usize) {
        if !t.owns(receiver) || receiver == source || !t.enabled {
            return;
        }
        match t.pair(source, receiver).and_then(|pair| t.make(pair)) {
            Some(c) => {
                t.connect(receiver, source, &c);
                self.put(receiver, source, c);
            }
            None => {
                if self.take(receiver, source).is_some() {
                    t.disconnect(receiver, source);
                }
            }
        }
    }
    /// Update an ordered connection without looking up the receiver's row again.
    fn update_connection(
        row: &mut Connections,
        t: &mut Tick,
        receiver: usize,
        source: usize,
        pair: Option<(PlayerRef<'_>, PlayerRef<'_>)>,
    ) {
        if receiver == source || !t.enabled {
            return;
        }
        if let Some(i) = row.index.get(source).copied().flatten() {
            let c = &mut row.ordered[i.get() - 1].1;
            let stamp = (t.serial as u32, t.view[receiver].map_or([0; 2], |v| v.0));
            // Entity.setPosRaw and ServerChunkCache.move can visit this pair
            // repeatedly with identical inputs. The first call already emitted
            // its packets; a changed receiver view must still be checked.
            if c.checked == stamp {
                return;
            }
            c.checked = stamp;
            // A connection to a player who isn't in the level is broken.
            if let Some(pair) = pair
                && !t.broken(c, pair)
            {
                t.update(c, pair);
                return;
            }
        }
        match pair.and_then(|pair| t.make(pair)) {
            Some(new) => {
                t.connect(receiver, source, &new);
                row.put(source, new);
            }
            None => {
                if row.remove(source).is_some() {
                    t.disconnect(receiver, source);
                }
            }
        }
    }
    /// `trackWaypoint`.
    fn track(&mut self, t: &mut Tick, source: usize) {
        if !self.waypoints.contains(&source) {
            self.waypoints.push(source);
        }
        for k in 0..self.players.len() {
            self.create_connection(t, self.players[k], source);
        }
    }
    /// `updateWaypoint`.
    fn update_waypoint(&mut self, t: &mut Tick, source: usize) {
        if !t.enabled || !self.waypoints.contains(&source) {
            return;
        }
        // The source is invariant across its receivers. References point into
        // the tick's input slice, so packet output can still be mutated.
        let source_player = t.player(&source);
        for k in 0..self.players.len() {
            let receiver = self.players[k];
            if !t.owns(receiver) {
                continue;
            }
            let pair = source_player.zip(t.player(&receiver));
            let row = &mut self.connections[receiver];
            Self::update_connection(row, t, receiver, source, pair);
        }
    }
    /// `untrackWaypoint`.
    fn untrack(&mut self, t: &mut Tick, source: usize) {
        for k in 0..self.players.len() {
            let receiver = self.players[k];
            if self.take(receiver, source).is_some() {
                t.disconnect(receiver, source);
            }
        }
        self.waypoints.retain(|w| *w != source);
    }
    /// `addPlayer`.
    fn add_player(&mut self, t: &mut Tick, player: usize, transmitting: bool) {
        if !self.players.contains(&player) {
            self.players.push(player);
        }
        for k in 0..self.waypoints.len() {
            self.create_connection(t, player, self.waypoints[k]);
        }
        if transmitting {
            self.track(t, player);
        }
    }
    /// `updatePlayer`.
    fn update_player(&mut self, t: &mut Tick, player: usize) {
        if !t.owns(player) || !t.enabled {
            return;
        }
        // Keep the receiver row and player lookup outside the source loop.
        let receiver = t.player(&player);
        let row = &mut self.connections[player];
        for &source in &self.waypoints {
            let pair = t.player(&source).zip(receiver);
            Self::update_connection(row, t, player, source, pair);
        }
    }
    /// `removePlayer`.
    fn remove_player(&mut self, t: &mut Tick, player: usize) {
        for (source, _) in std::mem::take(&mut self.connections[player]).ordered {
            t.disconnect(player, source);
        }
        self.untrack(t, player);
        self.players.retain(|p| *p != player);
    }
    /// `breakAllConnections`.
    fn break_all(&mut self, t: &mut Tick) {
        for &receiver in &self.players {
            for (source, _) in std::mem::take(&mut self.connections[receiver]).ordered {
                t.disconnect(receiver, source);
            }
        }
        for row in &mut self.connections {
            *row = Connections::default();
        }
    }

    /// One tick: players who left, joined, moved, hid or changed team, and the
    /// `locator_bar` game rule. `respawned` players are removed and added again,
    /// as `PlayerList.respawn` replaces the ServerPlayer. Returns the packets for
    /// each connection.
    pub fn tick(
        &mut self,
        players: &[Player],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        self.tick_level(players, &[], respawned, locator_bar)
    }

    /// [`Self::tick`] for one level of several: `away` supplies connections for
    /// receivers who left this level. Entries for current players are not queried.
    pub fn tick_level(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        let mut out = Vec::new();
        self.tick_level_into(players, away, respawned, locator_bar, &mut out);
        out
    }

    /// Append in vanilla order, retaining the caller's output allocation.
    pub fn tick_level_into(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
        out: &mut Vec<(u32, C)>,
    ) {
        self.tick_level_output(players, away, respawned, locator_bar, Output::Typed(out));
    }

    fn tick_level_output(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
        out: Output<'_>,
    ) {
        let mut geometry = std::mem::take(&mut self.geometry);
        geometry.clear();
        geometry.extend(players.iter().map(Geometry::new));
        self.tick_level_cached((players, &geometry), away, respawned, locator_bar, out);
        self.geometry = geometry;
    }

    fn tick_level_cached(
        &mut self,
        (players, geometry): (&[Player], &[Geometry]),
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
        out: Output<'_>,
    ) {
        self.tick = self.tick.wrapping_add(1);
        for f in &mut self.frame {
            f.input = None;
        }
        self.input_slots.clear();
        for (input, p) in players.iter().enumerate() {
            let slot = if let Some(&slot) = self.slots.get(&p.uuid) {
                slot
            } else {
                let slot = self.free.pop().unwrap_or_else(|| {
                    let slot = self.uuids.len();
                    self.uuids.push(None);
                    self.memo.push(None);
                    self.connections.push(Connections::default());
                    self.frame.push(SlotFrame::default());
                    self.view.push(None);
                    slot
                });
                self.slots.insert(p.uuid, slot);
                self.uuids[slot] = Some(p.uuid);
                slot
            };
            self.frame[slot] = SlotFrame {
                input: Some(input),
                owned: self.shard.owns(p.uuid),
            };
            self.input_slots.push(slot);
        }
        // Departed receivers still own their row until all disconnects have run.
        self.view.fill(None);
        let mut t = Tick {
            serial: self.tick,
            players,
            geometry,
            frame: std::mem::take(&mut self.frame),
            uuids: std::mem::take(&mut self.uuids),
            away,
            view: std::mem::take(&mut self.view),
            fresh: respawned,
            enabled: locator_bar,
            out,
            #[cfg(test)]
            count_only: self.count_only,
            #[cfg(test)]
            emitted: 0,
        };
        // Vanilla reads the rule when it connects; changing it breaks or remakes
        // every connection (MinecraftServer.onGameRuleChanged).
        let was = self.enabled.replace(locator_bar);
        for (slot, memo) in self.memo.iter().enumerate() {
            if let (Some(memo), Some(p)) = (memo, t.player(&slot)) {
                t.view[slot] = Some((memo.view_center, p.view_distance));
            }
        }
        // Players who left, and the ServerPlayer a respawn replaced.
        let gone: Vec<usize> = self
            .players
            .iter()
            .copied()
            .filter(|uuid| {
                t.frame[*uuid].input.is_none() || respawned.contains(&t.uuids[*uuid].unwrap())
            })
            .collect();
        let quiet = matches!(t.out, Output::Encoded { .. })
            && locator_bar
            && was == Some(true)
            && gone.is_empty()
            && respawned.is_empty()
            && players.iter().zip(&self.input_slots).all(|(p, &slot)| {
                self.memo[slot].as_ref().is_some_and(|memo| {
                    memo.transmitting == (p.transmit_range > 0.0) && memo.team == p.team
                })
            });
        if quiet {
            self.tick_rows(&mut t);
            self.finish_tick(t);
            return;
        }
        for uuid in gone {
            self.remove_player(&mut t, uuid);
            self.memo[uuid] = None;
        }
        if was == Some(true) && !locator_bar {
            self.break_all(&mut t);
        }
        // Players who joined (PlayerList.placeNewPlayer: addPlayer).
        for input in players {
            let p = t.player(&self.slots[&input.uuid]).unwrap();
            if self.memo[p.slot].is_some() {
                continue;
            }
            let center = p.chunk();
            t.view[p.slot] = Some((center, p.view_distance));
            let transmitting = p.transmit_range > 0.0;
            self.memo[p.slot] = Some(Memo {
                position: p.position,
                view_center: center,
                transmitting,
                team: p.team.clone(),
            });
            if p.receive_range > 0.0 {
                self.add_player(&mut t, p.slot, transmitting);
            } else if transmitting {
                self.track(&mut t, p.slot);
            }
        }
        if was == Some(false) && locator_bar {
            for player in self.players.clone() {
                self.update_player(&mut t, player);
            }
        }
        for input in players {
            let p = t.player(&self.slots[&input.uuid]).unwrap();
            let memo = self.memo[p.slot].as_ref().unwrap();
            // Entity.setPosRaw, then ServerChunkCache.move.
            if memo.position != p.position {
                let moved_view = memo.view_center != p.chunk();
                let transmitting = memo.transmitting;
                self.memo[p.slot].as_mut().unwrap().position = p.position;
                if !p.first_tick {
                    if transmitting {
                        self.update_waypoint(&mut t, p.slot);
                    }
                    if p.receive_range > 0.0 {
                        self.update_player(&mut t, p.slot);
                    }
                }
                if moved_view {
                    self.memo[p.slot].as_mut().unwrap().view_center = p.chunk();
                    t.view[p.slot] = Some((p.chunk(), p.view_distance));
                }
                if p.receive_range > 0.0 {
                    self.update_player(&mut t, p.slot);
                }
            }
            // LivingEntity.onAttributeUpdated(WAYPOINT_TRANSMIT_RANGE).
            let transmitting = p.transmit_range > 0.0;
            if transmitting != self.memo[p.slot].as_ref().unwrap().transmitting {
                self.memo[p.slot].as_mut().unwrap().transmitting = transmitting;
                if transmitting {
                    self.track(&mut t, p.slot);
                } else {
                    self.untrack(&mut t, p.slot);
                }
            }
            // ServerScoreboard: a team joined, left or recoloured remakes the connections.
            if p.team != self.memo[p.slot].as_ref().unwrap().team {
                self.memo[p.slot].as_mut().unwrap().team = p.team.clone();
                for k in 0..self.players.len() {
                    self.create_connection(&mut t, self.players[k], p.slot);
                }
            }
        }
        self.finish_tick(t);
    }

    /// The moves of a tick where nobody joined, left, hid or changed team:
    /// the phases of `tick_level_cached` by receiver rather than by source,
    /// walking one row at a time. A pair's visits are ordered only within its
    /// receiver's row, which is what its connection sees: the sources that
    /// moved before this player (`updateWaypoint`), its own move
    /// (`updatePlayer`, again with its new view), then those after it.
    fn tick_rows(&mut self, t: &mut Tick) {
        self.is_waypoint.clear();
        self.is_waypoint.resize(t.uuids.len(), false);
        for &source in &self.waypoints {
            self.is_waypoint[source] = true;
        }
        self.moved.clear();
        for (input, &slot) in self.input_slots.iter().enumerate() {
            let p = &t.players[input];
            let memo = self.memo[slot].as_ref().unwrap();
            if memo.position != p.position
                && !p.first_tick
                && memo.transmitting
                && self.is_waypoint[slot]
            {
                self.moved.push((input, slot));
            }
        }
        for &receiver in &self.players {
            if !t.owns(receiver) {
                continue;
            }
            let Some(r) = t.player(&receiver) else {
                continue;
            };
            let input = t.frame[receiver].input.unwrap_or(0);
            let split = self.moved.partition_point(|&(i, _)| i < input);
            let row = &mut self.connections[receiver];
            for &(_, source) in &self.moved[..split] {
                let pair = t.player(&source).zip(Some(r));
                Self::update_connection(row, t, receiver, source, pair);
            }
            let memo = self.memo[receiver].as_ref().unwrap();
            if memo.position != r.position {
                if !r.first_tick && r.receive_range > 0.0 {
                    for &source in &self.waypoints {
                        let pair = t.player(&source).zip(Some(r));
                        Self::update_connection(row, t, receiver, source, pair);
                    }
                }
                if memo.view_center != r.chunk() {
                    t.view[receiver] = Some((r.chunk(), r.view_distance));
                }
                if r.receive_range > 0.0 {
                    for &source in &self.waypoints {
                        let pair = t.player(&source).zip(Some(r));
                        Self::update_connection(row, t, receiver, source, pair);
                    }
                }
            }
            for &(_, source) in &self.moved[split..] {
                let pair = t.player(&source).zip(Some(r));
                Self::update_connection(row, t, receiver, source, pair);
            }
        }
        for (input, &slot) in self.input_slots.iter().enumerate() {
            let p = t.player(&slot).expect("in this tick's input");
            debug_assert_eq!(t.frame[slot].input, Some(input));
            let memo = self.memo[slot].as_mut().unwrap();
            memo.position = p.position;
            memo.view_center = p.chunk();
        }
        #[cfg(test)]
        {
            self.row_ticks += 1;
        }
    }

    /// The end of a tick's two ways: back to the manager what the tick took.
    fn finish_tick(&mut self, t: Tick) {
        #[cfg(test)]
        {
            self.emitted = t.emitted;
        }
        self.frame = t.frame;
        self.uuids = t.uuids;
        // Slot identity survives until all ordered removals have emitted bytes.
        // A slot is reused only after both its receiver row and source columns
        // have been cleared, so no connection can refer to a previous UUID.
        for slot in 0..self.uuids.len() {
            if self.memo[slot].is_none()
                && self.frame[slot].input.is_none()
                && let Some(uuid) = self.uuids[slot].take()
            {
                self.slots.remove(&uuid);
                debug_assert!(self.connections[slot].ordered.is_empty());
                debug_assert!(
                    self.connections
                        .iter()
                        .all(|r| r.index.get(slot).copied().flatten().is_none())
                );
                self.free.push(slot);
            }
        }
        self.view = t.view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Run explicitly in release mode to measure the moving 100-player locator workload.
    #[test]
    #[ignore]
    fn moving_players_benchmark() {
        use std::{hint::black_box, time::Instant};
        for spacing in [8.0, 512.0] {
            let initial: Vec<_> = (0..100)
                .map(|i| player(i + 1, i as u32, [i as f64 * spacing, 64.0, 0.0]))
                .collect();
            let mut samples = Vec::new();
            for _ in 0..7 {
                let mut players = initial.clone();
                let mut manager = Waypoints::default();
                black_box(manager.tick(&players, &[], true));
                let start = Instant::now();
                let mut packets = 0;
                for tick in 0..200 {
                    for p in &mut players {
                        p.position[0] += 0.2;
                        p.position[2] = (tick as f64 * 0.03 + p.uuid as f64).sin() * 8.0;
                    }
                    packets += black_box(manager.tick(black_box(&players), &[], true)).len();
                }
                samples.push(start.elapsed().as_secs_f64() * 1e6 / 200.0);
                black_box(packets);
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "moving_players spacing={spacing} median_us_per_tick={:.3} samples={samples:?}",
                samples[3]
            );
        }
    }

    /// The profiled population, with 300 walkers and 200 distant explorers.
    #[test]
    #[ignore]
    fn five_hundred_players_benchmark() {
        use std::{
            hash::{Hash, Hasher},
            hint::black_box,
            time::Instant,
        };
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut players: Vec<_> = (0..500)
                .map(|i| {
                    player(
                        i + 1,
                        i as u32,
                        [
                            if i < 300 {
                                (i % 40) as f64
                            } else {
                                i as f64 * 32.0
                            },
                            64.0,
                            0.0,
                        ],
                    )
                })
                .collect();
            let mut manager = Waypoints::default();
            black_box(manager.tick(&players, &[], true));
            let mut elapsed = std::time::Duration::ZERO;
            let mut packets = 0;
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            for tick in 0..40 {
                for p in &mut players {
                    p.position[0] += 0.2;
                    p.position[2] = (tick as f64 * 0.03 + p.uuid as f64).sin() * 8.0;
                }
                let start = Instant::now();
                let out = black_box(manager.tick(black_box(&players), &[], true));
                elapsed += start.elapsed();
                packets += out.len();
                for (conn, packet) in out {
                    conn.hash(&mut hash);
                    packet.encode().hash(&mut hash);
                }
            }
            samples.push(elapsed.as_secs_f64() * 1e3 / 40.0);
            println!(
                "500p packets={packets} hash={} ms_per_tick={:.3}",
                hash.finish(),
                samples.last().unwrap()
            );
        }
        samples.sort_by(f64::total_cmp);
        println!("500p median_ms_per_tick={:.3}", samples[2]);
    }

    /// Separate the pooled production pass from packet encoding and queueing.
    #[test]
    #[ignore]
    fn waypoint_pipeline_breakdown_benchmark() {
        use crate::lan::{Holding, PlayerSender};
        use std::{
            hash::{Hash, Hasher},
            hint::black_box,
            sync::{Arc, atomic::Ordering, mpsc},
            time::{Duration, Instant},
        };
        println!(
            "waypoint_layout clientbound_bytes={} tracked_waypoint_bytes={}",
            std::mem::size_of::<C>(),
            std::mem::size_of::<TrackedWaypoint>()
        );
        let mut expected: Option<(usize, u64)> = None;
        for mode in ["allocating", "pooled", "count-only"] {
            let mut samples = Vec::new();
            for _ in 0..5 {
                let mut players: Vec<_> = (0..500)
                    .map(|i| {
                        player(
                            i + 1,
                            i as u32,
                            [
                                if i < 300 {
                                    (i % 40) as f64
                                } else {
                                    i as f64 * 32.0
                                },
                                64.0,
                                0.0,
                            ],
                        )
                    })
                    .collect();
                let mut manager = Waypoints {
                    count_only: mode == "count-only",
                    ..Waypoints::default()
                };
                black_box(manager.tick(&players, &[], true));
                let mut out = if mode == "pooled" {
                    Vec::with_capacity(players.len() * players.len())
                } else {
                    Vec::new()
                };
                let mut deliveries: std::collections::HashMap<u32, Vec<Vec<u8>>> =
                    Default::default();
                let (tx, rx) = mpsc::channel();
                let senders: Vec<_> = (0..500)
                    .map(|_| PlayerSender::new(tx.clone(), Arc::default()))
                    .collect();
                let _holding = Holding::start();
                let (mut pass, mut encode, mut send) =
                    (Duration::ZERO, Duration::ZERO, Duration::ZERO);
                let mut packets = 0;
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                for tick in 0..40 {
                    for p in &mut players {
                        p.position[0] += 0.2;
                        p.position[2] = (tick as f64 * 0.03 + p.uuid as f64).sin() * 8.0;
                    }
                    if mode == "allocating" {
                        out = Vec::new();
                    }
                    let start = Instant::now();
                    if mode == "allocating" {
                        out = black_box(manager.tick(black_box(&players), &[], true));
                    } else {
                        manager.tick_level_into(black_box(&players), &[], &[], true, &mut out);
                    }
                    pass += start.elapsed();
                    packets += manager.emitted;
                    if mode == "count-only" {
                        continue;
                    }
                    assert_eq!(out.len(), manager.emitted);
                    let start = Instant::now();
                    for (conn, packet) in out.drain(..) {
                        deliveries.entry(conn).or_default().push(packet.encode());
                    }
                    encode += start.elapsed();
                    // Fingerprinting stays outside every measured phase.
                    for conn in 0..500u32 {
                        conn.hash(&mut hash);
                        if let Some(bodies) = deliveries.get(&conn) {
                            for body in bodies {
                                body.hash(&mut hash);
                            }
                        }
                    }
                    let start = Instant::now();
                    for (conn, sender) in senders.iter().enumerate() {
                        if let Some(bodies) = deliveries.get_mut(&(conn as u32)) {
                            sender.send_many(bodies.drain(..));
                        }
                    }
                    send += start.elapsed();
                    // Model writer-side disposal outside the tick's measured work.
                    for sender in &senders {
                        sender.flush();
                        sender.pending.store(0, Ordering::Relaxed);
                    }
                    rx.try_iter().for_each(drop);
                }
                let result = (packets, hash.finish());
                if mode == "count-only" {
                    assert_eq!(expected.unwrap().0, packets);
                } else {
                    assert_eq!(*expected.get_or_insert(result), result);
                }
                let ms = |d: Duration| d.as_secs_f64() * 1e3 / 40.0;
                let sample = [ms(pass), ms(encode), ms(send)];
                println!(
                    "waypoint_pipeline mode={mode} pass_ms={:.3} encode_ms={:.3} send_ms={:.3} packets={packets} hash={}",
                    sample[0], sample[1], sample[2], result.1
                );
                samples.push(sample);
            }
            samples.sort_by(|a, b| a[0].total_cmp(&b[0]));
            println!(
                "waypoint_pipeline mode={mode} median_pass_ms={:.3} encode_ms={:.3} send_ms={:.3}",
                samples[2][0], samples[2][1], samples[2][2]
            );
        }
    }

    /// Each connection's packets as its writer thread would send them.
    fn by_connection(out: Vec<(u32, Delivery)>) -> BTreeMap<u32, Vec<Vec<u8>>> {
        let mut conns: BTreeMap<u32, Vec<Vec<u8>>> = BTreeMap::new();
        for (conn, delivery) in out {
            let packets = conns.entry(conn).or_default();
            match delivery {
                Delivery::Local(w) => packets.push(C::TrackedWaypoint(w).encode()),
                Delivery::Encoded(body) => packets.extend(body.packets().map(<[u8]>::to_vec)),
            }
        }
        conns
    }

    fn serial_by_connection(out: Vec<(u32, C)>) -> BTreeMap<u32, Vec<Vec<u8>>> {
        let mut conns: BTreeMap<u32, Vec<Vec<u8>>> = BTreeMap::new();
        for (conn, packet) in out {
            conns.entry(conn).or_default().push(packet.encode());
        }
        conns
    }

    /// The row pass against the serial phases: moves over all three connection
    /// kinds and a view change, with a player on its first tick and one who
    /// only transmits, and the odd tick that takes the phases again.
    #[test]
    fn row_pass_matches_the_serial_phases_for_every_connection() {
        for threads in [1, 2, 4, 8] {
            let mut reference = Waypoints::default();
            let mut parallel = ParallelWaypoints {
                threads: Some(threads),
                ..ParallelWaypoints::default()
            };
            let mut players: Vec<_> = (1..=40u128)
                .map(|id| {
                    let x = if id % 4 == 0 {
                        id as f64 * 400.0
                    } else {
                        id as f64
                    };
                    let mut p = player(id, id as u32, [x, 64.0, 0.5]);
                    p.view_distance = if id % 3 == 0 { 32 } else { 4 };
                    p.receive_range = if id == 9 { 0.0 } else { PLAYER_RANGE };
                    p
                })
                .collect();
            let away = std::sync::Arc::new(Vec::new());
            let none = std::sync::Arc::new(Vec::new());
            for frame in 0..60u128 {
                for p in &mut players {
                    // Standing still every so often, and a few moving fast.
                    if (p.uuid + frame) % 7 != 0 {
                        p.position[0] += if p.uuid % 5 == 0 { 11.0 } else { 0.6 };
                        p.position[2] += if p.uuid % 2 == 0 { 1.3 } else { -0.7 };
                    }
                    p.first_tick = frame == 20 && p.uuid == 2;
                }
                if frame == 30 {
                    players[5].transmit_range = 0.0;
                }
                let expected = reference.tick_level(&players, &away, &[], true);
                let mut actual = Vec::new();
                parallel.tick_level_into(&mut players, &away, &none, true, Some(3), &mut actual);
                assert_eq!(
                    by_connection(actual),
                    serial_by_connection(expected),
                    "threads={threads} frame={frame}"
                );
            }
            let rows: usize = parallel.shards.iter().map(|s| s.manager.row_ticks).sum();
            assert_eq!(rows, threads * 58, "all but the first tick and the hiding");
        }
    }

    #[test]
    fn receiver_shards_preserve_each_connections_order_through_lifecycle_and_view_changes() {
        for threads in [2, 4, 8] {
            let mut reference = Waypoints::default();
            let mut parallel = ParallelWaypoints {
                threads: Some(1),
                ..ParallelWaypoints::default()
            };
            let mut players: Vec<_> = (1..=19)
                .map(|id| {
                    let mut p = player(id, id as u32, [id as f64 * 64.0, 64.0, 0.5]);
                    p.view_distance = if id % 3 == 0 { 64 } else { 4 };
                    p.first_tick = id % 5 == 0;
                    p.receive_range = if id % 7 == 0 { 0.0 } else { PLAYER_RANGE };
                    p
                })
                .collect();
            let away = std::sync::Arc::new(
                players
                    .iter()
                    .map(|p| (p.uuid, p.conn.unwrap()))
                    .collect::<Vec<_>>(),
            );
            for frame in 0..18 {
                // Split an already populated manager, moving its rows intact.
                if frame == 2 {
                    parallel.threads = Some(threads);
                }
                for p in &mut players {
                    p.position[0] += if frame % 3 == 0 { 17.0 } else { 0.2 };
                    p.position[2] += if p.uuid % 2 == 0 { 1.0 } else { -1.0 };
                    p.first_tick = frame == 4 && p.uuid == 2;
                    p.transmit_range = if (p.uuid + frame) % 5 == 0 {
                        0.0
                    } else {
                        PLAYER_RANGE
                    };
                    p.spectator = (p.uuid + frame) % 7 == 0;
                    p.team = if (p.uuid + frame) % 3 == 0 {
                        Some(("red".into(), Some(0xff5555)))
                    } else {
                        None
                    };
                }
                if frame == 6 {
                    players.retain(|p| p.uuid != 3 && p.uuid != 7);
                }
                if frame == 10 {
                    players.push(player(3, 3, [0.0, 64.0, 0.0]));
                }
                if frame == 17 {
                    players.clear();
                }
                let respawned: &[u128] = if frame == 8 { &[2, 5] } else { &[] };
                let enabled = frame != 5 && frame != 11;
                let expected = reference.tick_level(&players, &away, respawned, enabled);
                let mut actual = Vec::new();
                let respawned = std::sync::Arc::new(respawned.to_vec());
                let buffer = players.as_ptr();
                parallel.tick_level_into(
                    &mut players,
                    &away,
                    &respawned,
                    enabled,
                    Some(3),
                    &mut actual,
                );
                assert_eq!(players.as_ptr(), buffer);
                assert_eq!(std::sync::Arc::strong_count(&away), 1);
                assert_eq!(std::sync::Arc::strong_count(&respawned), 1);
                assert_eq!(
                    by_connection(actual),
                    serial_by_connection(expected),
                    "threads={threads} frame={frame}"
                );
            }
        }
    }

    #[test]
    fn recycled_slots_preserve_uuid_and_connection_order() {
        let mut reused = Waypoints::default();
        for generation in 0..12u128 {
            let mut players: Vec<_> = (0..6)
                .map(|i| player(100 + generation * 6 + i, i as u32, [i as f64, 64.0, 0.0]))
                .collect();
            let mut fresh = Waypoints::default();
            for frame in 0..8 {
                if frame == 2 {
                    players.remove(4);
                    players.remove(1);
                }
                if frame == 3 {
                    players.push(player(1000 + generation * 6, 1, [1.0, 64.0, 0.0]));
                    players.push(player(1001 + generation * 6, 4, [4.0, 64.0, 0.0]));
                }
                for p in &mut players {
                    p.position[0] += 0.2;
                    p.team = (frame == 3).then(|| ("red".into(), Some(0xff5555)));
                    p.transmit_range = if frame == 4 { 0.0 } else { PLAYER_RANGE };
                }
                let respawned = if frame == 6 {
                    vec![players[1].uuid]
                } else {
                    vec![]
                };
                let bytes = |out: Vec<(u32, C)>| {
                    out.into_iter()
                        .map(|(c, p)| (c, p.encode()))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    bytes(reused.tick(&players, &respawned, frame != 5)),
                    bytes(fresh.tick(&players, &respawned, frame != 5))
                );
            }
            let away: Vec<_> = players.iter().map(|p| (p.uuid, p.conn.unwrap())).collect();
            let bytes = |out: Vec<(u32, C)>| {
                out.into_iter()
                    .map(|(c, p)| (c, p.encode()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                bytes(reused.tick_level(&[], &away, &[], true)),
                bytes(fresh.tick_level(&[], &away, &[], true))
            );
            assert!(reused.slots.is_empty());
            assert_eq!(reused.uuids.len(), 6);
        }
    }

    #[test]
    fn borrowed_encoder_matches_protocol_codec() {
        for source in [0, u128::MAX] {
            for operation in [
                waypoint_operation::TRACK,
                waypoint_operation::UNTRACK,
                waypoint_operation::UPDATE,
            ] {
                for color in [None, Some(0), Some(-13_619_152), Some(0xabcdef)] {
                    for style in [
                        String::new(),
                        "example:custom_style".into(),
                        "x".repeat(127),
                        "x".repeat(128),
                    ] {
                        let icon = Icon { style, color };
                        for target in [
                            WaypointTarget::Empty,
                            WaypointTarget::Block([i32::MIN, -1, i32::MAX]),
                            WaypointTarget::Chunk([-129, 128]),
                            WaypointTarget::Azimuth(-1.25),
                            WaypointTarget::Azimuth(f32::from_bits(0x80000000)),
                            WaypointTarget::Azimuth(f32::from_bits(0x7fc00001)),
                        ] {
                            let mut bytes = Vec::new();
                            write_waypoint(&mut bytes, source, operation, &icon, target);
                            assert_eq!(bytes, waypoint(source, operation, &icon, target).encode());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn connection_index_preserves_order_after_remove_and_replace() {
        let mut row = Connections::default();
        let connection = |x| Connection {
            kind: Kind::Block([x, 0, 0]),
            icon: default_icon(),
            checked: (0, [0; 2]),
        };
        for id in 0..4 {
            row.put(id, connection(id as i32));
        }
        row.remove(1).unwrap();
        row.put(2, connection(20));
        row.put(1, connection(10));
        assert_eq!(
            row.ordered.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [0, 2, 3, 1]
        );
        for (i, (id, _)) in row.ordered.iter().enumerate() {
            assert_eq!(row.index[*id].map(|n| n.get() - 1), Some(i));
        }
        assert_eq!(row.remove(2).unwrap().kind, Kind::Block([20, 0, 0]));
        for id in [0, 3, 1] {
            assert!(row.remove(id).is_some());
        }
        assert!(row.index.iter().all(Option::is_none));
        assert!(row.remove(2).is_none());
    }

    fn player(uuid: u128, conn: u32, position: [f64; 3]) -> Player {
        Player {
            uuid,
            conn: Some(conn),
            position,
            spectator: false,
            transmit_range: PLAYER_RANGE,
            receive_range: PLAYER_RANGE,
            first_tick: false,
            view_distance: 4,
            team: None,
            icon: default_icon(),
        }
    }

    /// The packets to `conn`, as (operation, target).
    fn to(out: &[(u32, C)], conn: u32) -> Vec<(i32, WaypointTarget)> {
        out.iter()
            .filter(|(c, _)| *c == conn)
            .map(|(_, p)| match p {
                C::TrackedWaypoint(w) => (w.operation, w.target),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_new_player_shows_once_it_moves() {
        let mut w = Waypoints::default();
        let a = player(1, 10, [0.5, 100.0, 0.5]);
        let mut b = player(2, 20, [5.5, 100.0, 0.5]);
        b.first_tick = true;
        w.tick(std::slice::from_ref(&a), &[], true);
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        // B gets A at once; A sees nothing of B while B hasn't ticked.
        assert_eq!(to(&out, 20), [(0, WaypointTarget::Block([0, 100, 0]))]);
        assert!(to(&out, 10).is_empty());
        b.first_tick = false;
        assert!(w.tick(&[a.clone(), b.clone()], &[], true).is_empty());
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([6, 100, 0]))]);
        // One block: an update. Three: a new connection. Out of view: a chunk.
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(2, WaypointTarget::Block([7, 100, 0]))]);
        b.position[0] += 3.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([10, 100, 0]))]);
        b.position[0] = 100.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Chunk([6, 0]))]);
        // Really far: an angle.
        b.position[0] = 1000.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 10)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        // Hidden: untracked; the rule off: every connection broken.
        b.transmit_range = 0.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(1, WaypointTarget::Empty)]);
        let out = w.tick(&[a.clone(), b.clone()], &[], false);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
        assert!(to(&out, 10).is_empty());
        // Leaving untracks what was left.
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 20)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        let out = w.tick(&[b.clone()], &[], true);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
    }

    #[test]
    fn a_player_leaving_the_level_loses_its_waypoints() {
        let (a, b) = (
            player(1, 10, [0.5, 64.0, 0.5]),
            player(2, 20, [4.5, 64.0, 0.5]),
        );
        let mut w = Waypoints::default();
        w.tick(&[a.clone(), b.clone()], &[], true);
        let mut moved = b.clone();
        moved.position = [5.5, 64.0, 0.5];
        w.tick(&[a.clone(), moved], &[], true);
        // B goes to another level: A loses B, and B (reached through `away`) loses A.
        let out = w.tick_level(std::slice::from_ref(&a), &[(2, 20)], &[], true);
        assert_eq!(
            to(&out, 10),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
        assert_eq!(
            to(&out, 20),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
    }

    #[test]
    fn receiver_view_change_rechecks_a_pair_in_the_same_tick() {
        let mut w = Waypoints::default();
        let mut a = player(1, 10, [0.5, 64.0, 0.5]);
        let b = player(2, 20, [80.5, 64.0, 0.5]);
        w.tick(&[a.clone(), b.clone()], &[], true);
        a.position[0] += 16.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([80, 64, 0]))]);
        assert!(w.tick(&[a, b], &[], true).is_empty());
    }

    #[test]
    fn visible_far_connections_are_remade_on_every_trigger() {
        let mut w = Waypoints::default();
        let mut a = player(1, 10, [0.5, 64.0, 0.5]);
        a.view_distance = 64;
        let b = player(2, 20, [1000.5, 64.0, 0.5]);
        w.tick(&[a.clone(), b.clone()], &[], true);
        a.position[0] += 0.1;
        let out = w.tick(&[a, b], &[], true);
        // Both Entity.setPosRaw and ServerChunkCache.move remake this
        // azimuth: make chooses distance first, isBroken checks visibility.
        let packets = to(&out, 10);
        assert_eq!(packets.len(), 2);
        assert!(
            packets
                .iter()
                .all(|(op, target)| *op == 0 && matches!(target, WaypointTarget::Azimuth(_)))
        );
    }

    #[test]
    fn range_fast_path_keeps_distance_to_float_rounding() {
        for range in [
            0.0,
            1e-30,
            32.0,
            32.0000001,
            PLAYER_RANGE,
            f64::INFINITY,
            f64::NAN,
        ] {
            for distance in [
                0.0,
                range * 0.25,
                range * 0.99999999,
                range,
                range * 1.00000001,
            ] {
                if !distance.is_finite() {
                    continue;
                }
                let a = player(1, 10, [0.0, 64.0, 0.0]);
                let mut b = player(2, 20, [distance, 64.0, 0.0]);
                b.transmit_range = range;
                let ignored = f64::from(b.distance_to(&a)) >= range.min(a.receive_range);
                let expected = range > 0.0 && !ignored;
                let out = Waypoints::default().tick(&[a, b], &[], true);
                assert_eq!(
                    !to(&out, 10).is_empty(),
                    expected,
                    "range={range} distance={distance}"
                );
            }
        }
    }

    #[test]
    fn team_colours() {
        assert_eq!(team_rgb("red"), Some(0xFF5555));
        assert_eq!(team_rgb("black"), Some(0xFF30_3030_u32 as i32));
        assert_eq!(team_rgb("reset"), None);
    }
}
