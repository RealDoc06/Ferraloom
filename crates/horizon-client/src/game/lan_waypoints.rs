//! The locator bar of the players on a served world (lan/waypoints.rs): what
//! each player looks like to their level's waypoint manager this tick.
use super::{Game, lan::LanHost};
use horizon_client::{
    items::ItemStack,
    lan::{
        hub::HostState,
        waypoints::{self, HIDING_HEADS, Icon, PLAYER_RANGE},
    },
};

/// The host player's connection id in the waypoint managers: its packets
/// go to this client's locator bar rather than to a guest.
const HOST_CONN: u32 = u32::MAX;

/// `waypoint_transmit_range`: hidden by crouching, invisibility or a head worn.
fn transmit_range(crouching: bool, invisible: bool, head: Option<&ItemStack>) -> f64 {
    let hiding_head = head.is_some_and(|s| HIDING_HEADS.contains(&&*s.item));
    if crouching || invisible || hiding_head {
        0.0
    } else {
        PLAYER_RANGE
    }
}

/// Reused level snapshots and packet storage; no per-level player clones.
#[derive(Default)]
pub(super) struct WaypointTick {
    levels: std::collections::BTreeMap<String, LevelPlayers>,
    connections: std::sync::Arc<Vec<(u128, u32)>>,
    respawned: std::sync::Arc<Vec<u128>>,
    deliveries: std::collections::HashMap<u32, Vec<Vec<u8>>>,
    live_connections: std::collections::HashSet<u32>,
    out: Vec<(u32, waypoints::Delivery)>,
    #[cfg(feature = "profiler")]
    profile_ticks: u64,
}

#[derive(Default)]
struct LevelPlayers {
    players: Vec<waypoints::Player>,
    used: usize,
}

impl WaypointTick {
    fn push(&mut self, level: &str, player: waypoints::Player) {
        if let Some(conn) = player.conn {
            std::sync::Arc::get_mut(&mut self.connections)
                .expect("waypoint snapshot released")
                .push((player.uuid, conn));
        }
        if !self.levels.contains_key(level) {
            self.levels
                .insert(level.to_owned(), LevelPlayers::default());
        }
        let list = self.levels.get_mut(level).unwrap();
        if list.used == list.players.len() {
            list.players.push(player);
        } else {
            list.players[list.used] = player;
        }
        list.used += 1;
    }
}

impl Game<'_> {
    /// The player's team and its colour, as the locator bar shows it.
    fn waypoint_team(&self, name: &str) -> Option<(String, Option<i32>)> {
        let team = self.score.board.players_team(name)?;
        let color = team.color.as_deref().and_then(waypoints::team_rgb);
        Some((team.name.clone(), color))
    }

    /// Each level's `ServerWaypointManager` ticked with the players in that
    /// level: a player who changes level leaves the old one's manager
    /// (`ServerLevel.removePlayerImmediately`: untracked for everyone there)
    /// and joins the new one's (`addDuringTeleport`).
    pub(super) fn lan_update_waypoints(&mut self, host: &mut LanHost, state: Option<&HostState>) {
        let mut scratch = std::mem::take(&mut host.waypoint_tick);
        std::sync::Arc::get_mut(&mut scratch.connections)
            .expect("waypoint snapshot released")
            .clear();
        std::sync::Arc::get_mut(&mut scratch.respawned)
            .expect("waypoint snapshot released")
            .clear();
        for list in scratch.levels.values_mut() {
            list.used = 0;
        }
        if let Some(h) = state {
            let icon = self
                .cmds
                .waypoint_icons
                .get(&super::commands::Target::Player)
                .map(|i| Icon {
                    style: i.style.clone(),
                    color: i.color,
                })
                .unwrap_or_default();
            scratch.push(
                self.dim.id(),
                waypoints::Player {
                    uuid: h.uuid,
                    conn: Some(HOST_CONN),
                    position: h.position,
                    spectator: self.spectator(),
                    transmit_range: transmit_range(
                        h.sneaking,
                        self.potions.effects.get("invisibility").is_some(),
                        h.equipment[5].as_ref(),
                    ),
                    receive_range: PLAYER_RANGE,
                    first_tick: false,
                    view_distance: host.hub.config.view_distance,
                    team: self.waypoint_team(&h.name),
                    icon,
                },
            );
        }

        for g in host.hub.guests.iter().filter(|g| g.placed) {
            // PlayerList.respawn makes a new ServerPlayer.
            if g.dead {
                host.waypoint_dead.insert(g.uuid);
            } else if host.waypoint_dead.remove(&g.uuid) {
                std::sync::Arc::get_mut(&mut scratch.respawned)
                    .expect("waypoint snapshot released")
                    .push(g.uuid);
            }
            // Player.updatePlayerPose: the shift key crouches unless flying.
            let crouching = g.sneaking && !g.flying && !g.fall_flying && !g.sleeping;
            scratch.push(
                &g.dimension,
                waypoints::Player {
                    uuid: g.uuid,
                    conn: Some(g.conn),
                    position: g.position,
                    spectator: g.spectator(),
                    transmit_range: transmit_range(
                        crouching,
                        g.effects.get("invisibility").is_some(),
                        g.armor[0].as_ref(),
                    ),
                    receive_range: PLAYER_RANGE,
                    first_tick: g.ticks == 0,
                    view_distance: g.view_distance,
                    team: self.waypoint_team(&g.name),
                    icon: Icon::default(),
                },
            );
        }
        host.waypoint_dead
            .retain(|uuid| scratch.connections.iter().any(|(id, _)| id == uuid));
        let locator_bar = self.ui.rules.bool("locator_bar");
        for (level, list) in &mut scratch.levels {
            list.players.truncate(list.used);
            let manager = host.waypoints.entry(level.clone()).or_default();
            manager.tick_level_into(
                &mut list.players,
                &scratch.connections,
                &scratch.respawned,
                locator_bar,
                Some(HOST_CONN),
                &mut scratch.out,
            );
            if list.players.is_empty() {
                host.waypoints.remove(level);
            }
        }
        scratch.levels.retain(|_, list| list.used != 0);
        for (conn, delivery) in scratch.out.drain(..) {
            match delivery {
                waypoints::Delivery::Local(waypoint) => self.handle_waypoint(&waypoint),
                waypoints::Delivery::Encoded(body) => {
                    scratch.deliveries.entry(conn).or_default().push(body)
                }
            }
        }
        scratch.live_connections.clear();
        for g in &host.hub.guests {
            scratch.live_connections.insert(g.conn);
            if let Some(bodies) = scratch.deliveries.get_mut(&g.conn) {
                g.sender.send_many(bodies.drain(..));
            }
        }
        scratch
            .deliveries
            .retain(|conn, _| scratch.live_connections.contains(conn));
        #[cfg(feature = "profiler")]
        self.profile_waypoint_memory(&mut scratch, host);
        host.waypoint_tick = scratch;
    }

    /// Correlate retained waypoint headers with resident terrain and queued
    /// network bytes; RSS also includes world data and allocator free lists.
    #[cfg(feature = "profiler")]
    fn profile_waypoint_memory(&self, scratch: &mut WaypointTick, host: &LanHost) {
        scratch.profile_ticks += 1;
        if !scratch.profile_ticks.is_multiple_of(64) || scratch.connections.len() < 128 {
            return;
        }
        let output_headers =
            scratch.out.capacity() * std::mem::size_of::<(u32, waypoints::Delivery)>();
        let delivery_headers: usize = scratch
            .deliveries
            .values()
            .map(|b| b.capacity() * std::mem::size_of::<Vec<u8>>())
            .sum();
        let (pending_total, pending_max) = host
            .hub
            .guests
            .iter()
            .map(|g| g.sender.pending.load(std::sync::atomic::Ordering::Relaxed))
            .fold((0usize, 0usize), |(total, max), bytes| {
                (total + bytes, max.max(bytes))
            });
        let rss_kib = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("VmRSS:")
                        .and_then(|v| v.split_whitespace().next())
                        .and_then(|v| v.parse::<usize>().ok())
                })
            })
            .unwrap_or(0);
        eprintln!(
            "waypoint_host_memory tick={} players={} resident_chunks={} output_headers={output_headers} delivery_headers={delivery_headers} sender_pending_total={pending_total} sender_pending_max={pending_max} rss_kib={rss_kib}",
            scratch.profile_ticks,
            scratch.connections.len(),
            self.world.chunks.len()
        );
    }
}
