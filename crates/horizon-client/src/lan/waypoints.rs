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
    pub icon: Icon,
}

impl Player {
    fn block(&self) -> [i32; 3] {
        self.position.map(|v| v.floor() as i32)
    }
    fn chunk(&self) -> [i32; 2] {
        let [x, _, z] = self.block();
        [x >> 4, z >> 4]
    }
    fn distance_squared(&self, other: &Player) -> f64 {
        (0..3)
            .map(|k| (self.position[k] - other.position[k]).powi(2))
            .sum()
    }
    /// `Entity.distanceTo`.
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
    icon: Icon,
    /// Last tick and receiver view for which this connection was evaluated.
    checked: Option<(u64, [i32; 2])>,
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
    ordered: Vec<(u128, Connection)>,
    index: HashMap<u128, usize>,
}

impl Connections {
    fn put(&mut self, source: u128, connection: Connection) {
        if let Some(&i) = self.index.get(&source) {
            self.ordered[i].1 = connection;
        } else {
            self.index.insert(source, self.ordered.len());
            self.ordered.push((source, connection));
        }
    }

    fn remove(&mut self, source: u128) -> Option<Connection> {
        let i = self.index.remove(&source)?;
        let (_, connection) = self.ordered.remove(i);
        for (offset, (id, _)) in self.ordered[i..].iter().enumerate() {
            *self.index.get_mut(id).unwrap() = i + offset;
        }
        Some(connection)
    }
}

/// ServerWaypointManager for one level.
#[derive(Default)]
pub struct Waypoints {
    /// The transmitters, in the order they were tracked.
    waypoints: Vec<u128>,
    /// The receivers, in the order they joined.
    players: Vec<u128>,
    /// Connections by receiver, then source.
    connections: HashMap<u128, Connections>,
    memo: HashMap<u128, Memo>,
    /// The `locator_bar` game rule as last seen.
    enabled: Option<bool>,
    tick: u64,
    shard: Shard,
    orders: Vec<Order>,
    input_index: HashMap<u128, usize>,
    view: HashMap<u128, ([i32; 2], i32)>,
    #[cfg(test)]
    count_only: bool,
    #[cfg(test)]
    emitted: usize,
}

/// A packet's position in the serial manager's traversal, independent of shard.
type Order = (u64, usize, usize);

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

/// A local locator update or an already encoded network packet body.
pub enum Delivery {
    Local(TrackedWaypoint),
    Encoded(Vec<u8>),
}

fn encode_delivery(conn: u32, packet: C, local: Option<u32>) -> (u32, Delivery) {
    let delivery = if local == Some(conn) {
        let C::TrackedWaypoint(waypoint) = packet else {
            unreachable!("the waypoint manager emits only locator updates")
        };
        Delivery::Local(waypoint)
    } else {
        Delivery::Encoded(packet.encode())
    };
    (conn, delivery)
}

#[cfg(any(test, feature = "profiler"))]
#[derive(Clone, Copy, Debug, Default)]
struct ShardTime {
    update: std::time::Duration,
    encode: std::time::Duration,
    packets: usize,
    encoded_bytes: usize,
}

#[cfg(any(test, feature = "profiler"))]
#[derive(Clone, Copy, Debug, Default)]
struct PassTime {
    scope: std::time::Duration,
    wait: std::time::Duration,
    merge: std::time::Duration,
}

#[derive(Default)]
struct ReceiverShard {
    manager: Waypoints,
    out: Vec<(u32, C)>,
    ready: Vec<(u32, Delivery)>,
    #[cfg(any(test, feature = "profiler"))]
    time: ShardTime,
}

impl ReceiverShard {
    fn tick(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        enabled: bool,
        local: Option<u32>,
        encode: bool,
    ) {
        #[cfg(any(test, feature = "profiler"))]
        let start = std::time::Instant::now();
        self.manager
            .tick_level_into(players, away, respawned, enabled, &mut self.out);
        #[cfg(any(test, feature = "profiler"))]
        {
            self.time = ShardTime {
                update: start.elapsed(),
                packets: self.out.len(),
                ..ShardTime::default()
            };
        }
        if encode {
            #[cfg(any(test, feature = "profiler"))]
            let start = std::time::Instant::now();
            self.ready.extend(
                self.out
                    .drain(..)
                    .map(|(conn, packet)| encode_delivery(conn, packet, local)),
            );
            #[cfg(any(test, feature = "profiler"))]
            {
                self.time.encode = start.elapsed();
                self.time.encoded_bytes = self
                    .ready
                    .iter()
                    .map(|(_, d)| match d {
                        Delivery::Encoded(b) => b.len(),
                        Delivery::Local(_) => 0,
                    })
                    .sum();
            }
        }
    }
}

/// Every stream follows the serial traversal. Merge its order tags without
/// sorting or cloning bodies; encoded frames are much smaller than Clientbound.
fn merge_ordered<T>(mut streams: Vec<impl Iterator<Item = (Order, T)>>, out: &mut Vec<T>) {
    let mut heap = std::collections::BinaryHeap::new();
    let mut heads: Vec<_> = streams.iter_mut().map(Iterator::next).collect();
    for (index, head) in heads.iter().enumerate() {
        if let Some((order, _)) = head {
            heap.push(std::cmp::Reverse((*order, index)));
        }
    }
    while let Some(std::cmp::Reverse((_, index))) = heap.pop() {
        out.push(heads[index].take().unwrap().1);
        heads[index] = streams[index].next();
        if let Some((order, _)) = &heads[index] {
            heap.push(std::cmp::Reverse((*order, index)));
        }
    }
}

#[cfg(feature = "profiler")]
#[derive(Default)]
struct WaypointProfile {
    ticks: u64,
    peak_headers: usize,
    peak_rows: usize,
}

#[cfg(feature = "profiler")]
impl Drop for WaypointProfile {
    fn drop(&mut self) {
        eprintln!(
            "waypoint_memory ticks={} peak_vector_bytes={} peak_connection_rows={}",
            self.ticks, self.peak_headers, self.peak_rows
        );
    }
}

/// Receiver rows of ServerWaypointManager evaluated on the server's helper lane.
/// Each shard replays the same lifecycle and view phases over immutable inputs;
/// only its connection rows mutate. The merge restores the serial packet order.
pub struct ParallelWaypoints {
    shards: Vec<ReceiverShard>,
    #[cfg(test)]
    threads: Option<usize>,
    #[cfg(test)]
    encode_after_join: bool,
    #[cfg(test)]
    legacy_out: Vec<(u32, C)>,
    #[cfg(any(test, feature = "profiler"))]
    time: PassTime,
    #[cfg(feature = "profiler")]
    profile: WaypointProfile,
}

impl Default for ParallelWaypoints {
    fn default() -> Self {
        Self {
            shards: vec![ReceiverShard::default()],
            #[cfg(test)]
            threads: None,
            #[cfg(test)]
            encode_after_join: false,
            #[cfg(test)]
            legacy_out: Vec::new(),
            #[cfg(any(test, feature = "profiler"))]
            time: PassTime::default(),
            #[cfg(feature = "profiler")]
            profile: WaypointProfile::default(),
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
        for (receiver, row) in connections {
            self.shards[Shard::owner(receiver, count)]
                .manager
                .connections
                .insert(receiver, row);
        }
    }

    /// Join encoded output in the original global order. The local receiver's
    /// typed updates are applied by the caller only after the complete pass.
    pub fn tick_level_into(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
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
        #[cfg(test)]
        let encode_after_join = self.encode_after_join;
        #[cfg(not(test))]
        let encode_after_join = false;
        #[cfg(any(test, feature = "profiler"))]
        let start = std::time::Instant::now();
        #[cfg(any(test, feature = "profiler"))]
        let main_done;
        if self.shards.len() == 1 {
            self.shards[0].tick(
                players,
                away,
                respawned,
                locator_bar,
                local,
                !encode_after_join,
            );
            #[cfg(any(test, feature = "profiler"))]
            {
                main_done = start.elapsed();
            }
        } else {
            let (tick_shard, helpers) = self.shards.split_first_mut().unwrap();
            #[cfg(any(test, feature = "profiler"))]
            let mut done = std::time::Duration::ZERO;
            std::thread::scope(|scope| {
                for shard in helpers {
                    std::thread::Builder::new()
                        .name("waypoint-worker".into())
                        .spawn_scoped(scope, move || {
                            horizon_threads::worker_affinity();
                            shard.tick(
                                players,
                                away,
                                respawned,
                                locator_bar,
                                local,
                                !encode_after_join,
                            );
                        })
                        .expect("start waypoint worker");
                }
                tick_shard.tick(
                    players,
                    away,
                    respawned,
                    locator_bar,
                    local,
                    !encode_after_join,
                );
                #[cfg(any(test, feature = "profiler"))]
                {
                    done = start.elapsed();
                }
            });
            #[cfg(any(test, feature = "profiler"))]
            {
                main_done = done;
            }
        }
        #[cfg(any(test, feature = "profiler"))]
        let merge_start = std::time::Instant::now();
        #[cfg(test)]
        if encode_after_join {
            if self.shards.len() == 1 {
                self.legacy_out.append(&mut self.shards[0].out);
            } else {
                let streams = self
                    .shards
                    .iter_mut()
                    .map(|s| s.manager.orders.drain(..).zip(s.out.drain(..)))
                    .collect();
                merge_ordered(streams, &mut self.legacy_out);
            }
            out.extend(
                self.legacy_out
                    .drain(..)
                    .map(|(c, p)| encode_delivery(c, p, local)),
            );
        }
        if !encode_after_join {
            if self.shards.len() == 1 {
                out.append(&mut self.shards[0].ready);
            } else {
                let streams = self
                    .shards
                    .iter_mut()
                    .map(|s| s.manager.orders.drain(..).zip(s.ready.drain(..)))
                    .collect();
                merge_ordered(streams, out);
            }
        }
        #[cfg(any(test, feature = "profiler"))]
        {
            self.time = PassTime {
                scope: merge_start.duration_since(start),
                wait: merge_start.duration_since(start).saturating_sub(main_done),
                merge: merge_start.elapsed(),
            };
        }
        #[cfg(feature = "profiler")]
        self.record_profile(players.len(), out.capacity());
    }

    #[cfg(any(test, feature = "profiler"))]
    fn retained_vectors(&self, output_capacity: usize) -> (usize, usize, usize) {
        let mut bytes = output_capacity * std::mem::size_of::<(u32, Delivery)>();
        #[cfg(test)]
        {
            bytes += self.legacy_out.capacity() * std::mem::size_of::<(u32, C)>();
        }
        let (mut rows, mut row_capacity) = (0, 0);
        for shard in &self.shards {
            bytes += shard.out.capacity() * std::mem::size_of::<(u32, C)>()
                + shard.ready.capacity() * std::mem::size_of::<(u32, Delivery)>()
                + shard.manager.orders.capacity() * std::mem::size_of::<Order>();
            for row in shard.manager.connections.values() {
                rows += row.ordered.len();
                row_capacity += row.ordered.capacity();
                bytes += row.ordered.capacity() * std::mem::size_of::<(u128, Connection)>();
            }
        }
        (bytes, rows, row_capacity)
    }

    #[cfg(feature = "profiler")]
    fn record_profile(&mut self, players: usize, output_capacity: usize) {
        let (bytes, rows, row_capacity) = self.retained_vectors(output_capacity);
        self.profile.ticks += 1;
        self.profile.peak_headers = self.profile.peak_headers.max(bytes);
        self.profile.peak_rows = self.profile.peak_rows.max(rows);
        if players >= 128 && self.profile.ticks.is_multiple_of(64) {
            let work: Vec<_> = self
                .shards
                .iter()
                .map(|s| {
                    [
                        s.time.update.as_secs_f64() * 1000.0,
                        s.time.encode.as_secs_f64() * 1000.0,
                        s.time.packets as f64,
                    ]
                })
                .collect();
            let encoded_bytes: usize = self.shards.iter().map(|s| s.time.encoded_bytes).sum();
            eprintln!(
                "waypoint_pass players={players} tick={} scope_ms={:.3} join_wait_ms={:.3} merge_ms={:.3} shard_update_encode_packets={work:?} vector_bytes={bytes} rows={rows} row_capacity={row_capacity} encoded_bytes={encoded_bytes}",
                self.profile.ticks,
                self.time.scope.as_secs_f64() * 1000.0,
                self.time.wait.as_secs_f64() * 1000.0,
                self.time.merge.as_secs_f64() * 1000.0
            );
        }
    }
}

/// One tick's players, and the packets for their connections.
struct Tick<'a> {
    serial: u64,
    players: &'a [Player],
    index: HashMap<u128, usize>,
    /// Fallback connections for receivers absent from `players`: a player who
    /// left this level still loses its waypoints (`removePlayer` disconnects them).
    away: &'a [(u128, u32)],
    view: HashMap<u128, ([i32; 2], i32)>,
    /// Players a respawn replaced this tick: a new ServerPlayer, on its first tick.
    fresh: &'a [u128],
    enabled: bool,
    out: &'a mut Vec<(u32, C)>,
    orders: &'a mut Vec<Order>,
    order: Order,
    sharded: bool,
    #[cfg(test)]
    count_only: bool,
    #[cfg(test)]
    emitted: usize,
}

impl<'a> Tick<'a> {
    fn phase(&mut self) {
        self.order = (self.order.0 + 1, 0, 0);
    }

    fn player(&self, uuid: &u128) -> Option<&'a Player> {
        self.index.get(uuid).map(|&i| &self.players[i])
    }
    fn send(
        &mut self,
        receiver: u128,
        source: u128,
        operation: i32,
        icon: &Icon,
        target: WaypointTarget,
    ) {
        let Some(conn) = self.player(&receiver).and_then(|p| p.conn).or_else(|| {
            self.away
                .iter()
                .find(|(uuid, _)| *uuid == receiver)
                .map(|&(_, conn)| conn)
        }) else {
            return;
        };
        self.emit(conn, source, operation, icon, target);
    }
    fn emit(
        &mut self,
        conn: u32,
        source: u128,
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
        if self.sharded {
            self.orders.push(self.order);
        }
        self.out
            .push((conn, waypoint(source, operation, icon, target)));
    }
    fn connect(&mut self, receiver: u128, source: u128, c: &Connection) {
        let target = match c.kind {
            Kind::Block(p) => WaypointTarget::Block(p),
            Kind::Chunk(p) => WaypointTarget::Chunk(p),
            Kind::Azimuth(a) => WaypointTarget::Azimuth(a),
        };
        self.send(receiver, source, waypoint_operation::TRACK, &c.icon, target);
    }
    /// `ClientboundTrackedWaypointPacket.removeWaypoint`: an empty waypoint with the null icon.
    fn disconnect(&mut self, receiver: u128, source: u128) {
        self.send(
            receiver,
            source,
            waypoint_operation::UNTRACK,
            &Icon::default(),
            WaypointTarget::Empty,
        );
    }
    /// `ChunkTrackingView.isInViewDistance` of the receiver's view.
    fn chunk_visible(&self, [x, z]: [i32; 2], receiver: u128) -> bool {
        let Some(&([cx, cz], r)) = self.view.get(&receiver) else {
            return false;
        };
        let dx = i64::from(((x - cx).abs() - 1).max(0));
        let dz = i64::from(((z - cz).abs() - 1).max(0));
        dx * dx + dz * dz < i64::from(r) * i64::from(r)
    }
    /// `WaypointTransmitter.doesSourceIgnoreReceiver`.
    fn ignores(&self, source: &Player, receiver: &Player) -> bool {
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
    fn really_far(source: &Player, receiver: &Player) -> bool {
        source.distance_to(receiver) > REALLY_FAR
    }
    /// `EntityAzimuthConnection`: the angle of the receiver seen from the source,
    /// `Mth.atan2` of the offset turned clockwise.
    fn azimuth(source: &Player, receiver: &Player) -> f32 {
        let d = [0, 1, 2].map(|k| receiver.position[k] - source.position[k]);
        horizon_core::geometry::trig::atan2(d[0], -d[2]) as f32
    }
    /// The source and the receiver, if both are in the level this tick.
    fn pair(&self, source: u128, receiver: u128) -> Option<(&'a Player, &'a Player)> {
        Some((self.player(&source)?, self.player(&receiver)?))
    }
    /// `LivingEntity.makeWaypointConnectionWith`.
    fn make(&self, (s, r): (&Player, &Player)) -> Option<Connection> {
        if s.first_tick || self.fresh.contains(&s.uuid) || s.uuid == r.uuid || self.ignores(s, r) {
            return None;
        }
        // Waypoint.Icon.cloneAndAssignStyle: the icon's colour, else the team's.
        let mut icon = s.icon.clone();
        if icon.color.is_none() {
            icon.color = s.team.as_ref().and_then(|t| t.1);
        }
        let kind = if Self::really_far(s, r) {
            Kind::Azimuth(Self::azimuth(s, r))
        } else if !self.chunk_visible(s.chunk(), r.uuid) {
            Kind::Chunk(s.chunk())
        } else {
            Kind::Block(s.block())
        };
        Some(Connection {
            kind,
            icon,
            checked: None,
        })
    }
    /// `Connection.isBroken` for each kind.
    fn broken(&self, c: &Connection, (s, r): (&Player, &Player)) -> bool {
        let receiver = r.uuid;
        match c.kind {
            Kind::Block(last) => {
                let now = s.block();
                (0..3).map(|k| (now[k] - last[k]).abs()).sum::<i32>() > 1 || self.ignores(s, r)
            }
            Kind::Chunk(last) => {
                let now = s.chunk();
                let moved = (now[0] - last[0]).abs().max((now[1] - last[1]).abs());
                moved > 1 || self.ignores(s, r) || self.chunk_visible(last, receiver)
            }
            Kind::Azimuth(_) => {
                self.ignores(s, r)
                    || self.chunk_visible(s.chunk(), receiver)
                    || !Self::really_far(s, r)
            }
        }
    }
    /// `Connection.update`: the packet when the waypoint moved.
    fn update(&mut self, c: &mut Connection, (s, r): (&Player, &Player)) {
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
                let now = Self::azimuth(s, r);
                if (now - *last).abs() <= AZIMUTH_STEP {
                    return;
                }
                *last = now;
                WaypointTarget::Azimuth(now)
            }
        };
        if let Some(conn) = r.conn {
            self.emit(conn, s.uuid, waypoint_operation::UPDATE, &c.icon, target);
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
    fn put(&mut self, receiver: u128, source: u128, c: Connection) {
        self.connections.entry(receiver).or_default().put(source, c);
    }
    fn take(&mut self, receiver: u128, source: u128) -> Option<Connection> {
        self.connections.get_mut(&receiver)?.remove(source)
    }

    /// `createConnection`.
    fn create_connection(&mut self, t: &mut Tick, receiver: u128, source: u128) {
        if !self.shard.owns(receiver) || receiver == source || !t.enabled {
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
        receiver: u128,
        source: u128,
        pair: Option<(&Player, &Player)>,
    ) {
        if receiver == source || !t.enabled {
            return;
        }
        if let Some(i) = row.index.get(&source).copied() {
            let c = &mut row.ordered[i].1;
            let stamp = (t.serial, t.view.get(&receiver).map_or([0; 2], |v| v.0));
            // Entity.setPosRaw and ServerChunkCache.move can visit this pair
            // repeatedly with identical inputs. The first call already emitted
            // its packets; a changed receiver view must still be checked.
            if c.checked == Some(stamp) {
                return;
            }
            c.checked = Some(stamp);
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
    fn track(&mut self, t: &mut Tick, source: u128) {
        t.phase();
        if !self.waypoints.contains(&source) {
            self.waypoints.push(source);
        }
        for k in 0..self.players.len() {
            t.order.1 = k;
            self.create_connection(t, self.players[k], source);
        }
    }
    /// `updateWaypoint`.
    fn update_waypoint(&mut self, t: &mut Tick, source: u128) {
        t.phase();
        if !t.enabled || !self.waypoints.contains(&source) {
            return;
        }
        // The source is invariant across its receivers. References point into
        // the tick's input slice, so packet output can still be mutated.
        let source_player = t.player(&source);
        for k in 0..self.players.len() {
            let receiver = self.players[k];
            if !self.shard.owns(receiver) {
                continue;
            }
            t.order.1 = k;
            let pair = source_player.zip(t.player(&receiver));
            let row = self.connections.entry(receiver).or_default();
            Self::update_connection(row, t, receiver, source, pair);
        }
    }
    /// `untrackWaypoint`.
    fn untrack(&mut self, t: &mut Tick, source: u128) {
        t.phase();
        for k in 0..self.players.len() {
            t.order.1 = k;
            let receiver = self.players[k];
            if self.take(receiver, source).is_some() {
                t.disconnect(receiver, source);
            }
        }
        self.waypoints.retain(|w| *w != source);
    }
    /// `addPlayer`.
    fn add_player(&mut self, t: &mut Tick, player: u128, transmitting: bool) {
        t.phase();
        if !self.players.contains(&player) {
            self.players.push(player);
        }
        for k in 0..self.waypoints.len() {
            t.order.1 = k;
            self.create_connection(t, player, self.waypoints[k]);
        }
        if transmitting {
            self.track(t, player);
        }
    }
    /// `updatePlayer`.
    fn update_player(&mut self, t: &mut Tick, player: u128) {
        t.phase();
        if !self.shard.owns(player) || !t.enabled {
            return;
        }
        // Keep the receiver row and player lookup outside the source loop.
        let receiver = t.player(&player);
        let row = self.connections.entry(player).or_default();
        for (k, &source) in self.waypoints.iter().enumerate() {
            t.order.1 = k;
            let pair = t.player(&source).zip(receiver);
            Self::update_connection(row, t, player, source, pair);
        }
    }
    /// `removePlayer`.
    fn remove_player(&mut self, t: &mut Tick, player: u128) {
        t.phase();
        for (k, (source, _)) in self
            .connections
            .remove(&player)
            .unwrap_or_default()
            .ordered
            .into_iter()
            .enumerate()
        {
            t.order.1 = k;
            t.disconnect(player, source);
        }
        self.untrack(t, player);
        self.players.retain(|p| *p != player);
    }
    /// `breakAllConnections`.
    fn break_all(&mut self, t: &mut Tick) {
        t.phase();
        for (k, &receiver) in self.players.iter().enumerate() {
            t.order.1 = k;
            for (j, (source, _)) in self
                .connections
                .remove(&receiver)
                .unwrap_or_default()
                .ordered
                .into_iter()
                .enumerate()
            {
                t.order.2 = j;
                t.disconnect(receiver, source);
            }
        }
        self.connections.clear();
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
        self.tick = self.tick.wrapping_add(1);
        self.input_index.clear();
        self.input_index
            .extend(players.iter().enumerate().map(|(i, p)| (p.uuid, i)));
        self.view.clear();
        self.orders.clear();
        let mut orders = std::mem::take(&mut self.orders);
        let mut t = Tick {
            serial: self.tick,
            players,
            index: std::mem::take(&mut self.input_index),
            away,
            view: std::mem::take(&mut self.view),
            fresh: respawned,
            enabled: locator_bar,
            out,
            orders: &mut orders,
            order: (0, 0, 0),
            sharded: self.shard.count > 1,
            #[cfg(test)]
            count_only: self.count_only,
            #[cfg(test)]
            emitted: 0,
        };
        // Vanilla reads the rule when it connects; changing it breaks or remakes
        // every connection (MinecraftServer.onGameRuleChanged).
        let was = self.enabled.replace(locator_bar);
        for (uuid, memo) in &self.memo {
            if let Some(p) = t.player(uuid) {
                t.view.insert(*uuid, (memo.view_center, p.view_distance));
            }
        }
        // Players who left, and the ServerPlayer a respawn replaced.
        let gone: Vec<u128> = self
            .players
            .iter()
            .copied()
            .filter(|uuid| !t.index.contains_key(uuid) || respawned.contains(uuid))
            .collect();
        for uuid in gone {
            self.remove_player(&mut t, uuid);
            self.memo.remove(&uuid);
        }
        if was == Some(true) && !locator_bar {
            self.break_all(&mut t);
        }
        // Players who joined (PlayerList.placeNewPlayer: addPlayer).
        for p in players {
            if self.memo.contains_key(&p.uuid) {
                continue;
            }
            let center = p.chunk();
            t.view.insert(p.uuid, (center, p.view_distance));
            let transmitting = p.transmit_range > 0.0;
            self.memo.insert(
                p.uuid,
                Memo {
                    position: p.position,
                    view_center: center,
                    transmitting,
                    team: p.team.clone(),
                },
            );
            if p.receive_range > 0.0 {
                self.add_player(&mut t, p.uuid, transmitting);
            } else if transmitting {
                self.track(&mut t, p.uuid);
            }
        }
        if was == Some(false) && locator_bar {
            for player in self.players.clone() {
                self.update_player(&mut t, player);
            }
        }
        for p in players {
            let memo = &self.memo[&p.uuid];
            // Entity.setPosRaw, then ServerChunkCache.move.
            if memo.position != p.position {
                let moved_view = memo.view_center != p.chunk();
                let transmitting = memo.transmitting;
                self.memo.get_mut(&p.uuid).unwrap().position = p.position;
                if !p.first_tick {
                    if transmitting {
                        self.update_waypoint(&mut t, p.uuid);
                    }
                    if p.receive_range > 0.0 {
                        self.update_player(&mut t, p.uuid);
                    }
                }
                if moved_view {
                    self.memo.get_mut(&p.uuid).unwrap().view_center = p.chunk();
                    t.view.insert(p.uuid, (p.chunk(), p.view_distance));
                }
                if p.receive_range > 0.0 {
                    self.update_player(&mut t, p.uuid);
                }
            }
            // LivingEntity.onAttributeUpdated(WAYPOINT_TRANSMIT_RANGE).
            let transmitting = p.transmit_range > 0.0;
            if transmitting != self.memo[&p.uuid].transmitting {
                self.memo.get_mut(&p.uuid).unwrap().transmitting = transmitting;
                if transmitting {
                    self.track(&mut t, p.uuid);
                } else {
                    self.untrack(&mut t, p.uuid);
                }
            }
            // ServerScoreboard: a team joined, left or recoloured remakes the connections.
            if p.team != self.memo[&p.uuid].team {
                self.memo.get_mut(&p.uuid).unwrap().team = p.team.clone();
                t.phase();
                for k in 0..self.players.len() {
                    t.order.1 = k;
                    self.create_connection(&mut t, self.players[k], p.uuid);
                }
            }
        }
        #[cfg(test)]
        {
            self.emitted = t.emitted;
        }
        self.input_index = t.index;
        self.view = t.view;
        self.orders = orders;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn receiver_shards_preserve_global_order_through_lifecycle_and_view_changes() {
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
            let away: Vec<_> = players.iter().map(|p| (p.uuid, p.conn.unwrap())).collect();
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
                parallel.tick_level_into(&players, &away, respawned, enabled, Some(3), &mut actual);
                let encode_actual = actual
                    .into_iter()
                    .map(|(c, d)| {
                        (
                            c,
                            match d {
                                Delivery::Encoded(b) => b,
                                Delivery::Local(w) => C::TrackedWaypoint(w).encode(),
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let encode_expected = expected
                    .into_iter()
                    .map(|(c, p)| (c, p.encode()))
                    .collect::<Vec<_>>();
                assert_eq!(
                    encode_actual, encode_expected,
                    "threads={threads} frame={frame}"
                );
            }
        }
    }

    /// Run under a CPU taskset: this uses the production tick/helper affinity
    /// plan, including dispatch and the globally ordered join.
    #[test]
    #[ignore]
    fn parallel_waypoints_benchmark() {
        use std::{
            hash::{Hash, Hasher},
            time::{Duration, Instant},
        };
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
        horizon_threads::configure_server_affinity();
        let helpers = horizon_threads::worker_threads().unwrap_or(0);
        horizon_threads::tick_affinity();
        println!("waypoint_layout allocated_cpus={cpus} helpers={helpers} tick_shards=1");
        for population in [150, 500] {
            let mut expected = None;
            for mode in ["serial", "after-join", "worker-encode"] {
                let mut samples = Vec::new();
                for _ in 0..5 {
                    let normal = if population == 500 { 300 } else { 100 };
                    let mut players: Vec<_> = (0..population)
                        .map(|i| {
                            player(
                                i + 1,
                                i as u32,
                                [
                                    if i < normal {
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
                    let mut manager = ParallelWaypoints {
                        threads: (mode == "serial").then_some(1),
                        encode_after_join: mode != "worker-encode",
                        ..ParallelWaypoints::default()
                    };
                    let mut out = Vec::new();
                    manager.tick_level_into(&players, &[], &[], true, None, &mut out);
                    out.clear();
                    let mut elapsed = Duration::ZERO;
                    let mut packets = 0;
                    let mut scope_time = Duration::ZERO;
                    let mut wait_time = Duration::ZERO;
                    let mut merge_time = Duration::ZERO;
                    let mut shard_work = vec![Duration::ZERO; manager.shards.len()];
                    let mut shard_encode = vec![Duration::ZERO; manager.shards.len()];
                    let mut global = std::collections::hash_map::DefaultHasher::new();
                    let mut receivers: Vec<_> = (0..population)
                        .map(|_| std::collections::hash_map::DefaultHasher::new())
                        .collect();
                    for tick in 0..40 {
                        for p in &mut players {
                            p.position[0] += 0.2;
                            p.position[2] = (tick as f64 * 0.03 + p.uuid as f64).sin() * 8.0;
                        }
                        let start = Instant::now();
                        manager.tick_level_into(
                            std::hint::black_box(&players),
                            &[],
                            &[],
                            true,
                            None,
                            &mut out,
                        );
                        elapsed += start.elapsed();
                        scope_time += manager.time.scope;
                        wait_time += manager.time.wait;
                        merge_time += manager.time.merge;
                        for (i, shard) in manager.shards.iter().enumerate() {
                            shard_work[i] += shard.time.update;
                            shard_encode[i] += shard.time.encode;
                        }
                        packets += out.len();
                        for (conn, packet) in out.drain(..) {
                            let bytes = match packet {
                                Delivery::Encoded(b) => b,
                                Delivery::Local(w) => C::TrackedWaypoint(w).encode(),
                            };
                            conn.hash(&mut global);
                            bytes.hash(&mut global);
                            bytes.hash(&mut receivers[conn as usize]);
                        }
                    }
                    let fingerprint = (
                        packets,
                        global.finish(),
                        receivers
                            .into_iter()
                            .map(|h| h.finish())
                            .collect::<Vec<_>>(),
                    );
                    assert_eq!(*expected.get_or_insert(fingerprint.clone()), fingerprint);
                    let ms = elapsed.as_secs_f64() * 1e3 / 40.0;
                    samples.push(ms);
                    let per_tick = |d: Duration| d.as_secs_f64() * 1000.0 / 40.0;
                    let update: Vec<_> = shard_work.into_iter().map(per_tick).collect();
                    let encode: Vec<_> = shard_encode.into_iter().map(per_tick).collect();
                    println!(
                        "waypoint_stages population={population} mode={mode} scope_ms={:.3} wait_ms={:.3} merge_encode_ms={:.3} update={update:?} encode={encode:?} retained={:?}",
                        per_tick(scope_time),
                        per_tick(wait_time),
                        per_tick(merge_time),
                        manager.retained_vectors(out.capacity())
                    );
                    println!(
                        "waypoint_parallel population={population} mode={mode} shards={} ms={ms:.3} packets={packets} global_hash={}",
                        manager.shards.len(),
                        fingerprint.1
                    );
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "waypoint_parallel population={population} mode={mode} median_ms={:.3} samples={samples:?}",
                    samples[2]
                );
            }
        }
    }

    #[test]
    fn connection_index_preserves_order_after_remove_and_replace() {
        let mut row = Connections::default();
        let connection = |x| Connection {
            kind: Kind::Block([x, 0, 0]),
            icon: Icon::default(),
            checked: None,
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
            assert_eq!(row.index[id], i);
        }
        assert_eq!(row.remove(2).unwrap().kind, Kind::Block([20, 0, 0]));
        for id in [0, 3, 1] {
            assert!(row.remove(id).is_some());
        }
        assert!(row.index.is_empty());
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
            icon: Icon::default(),
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
