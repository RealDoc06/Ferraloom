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
    connections: Vec<(u128, u32)>,
    respawned: Vec<u128>,
    out: Vec<(u32, horizon_protocol::packets::play::Clientbound)>,
}

#[derive(Default)]
struct LevelPlayers {
    players: Vec<waypoints::Player>,
    used: usize,
}

impl WaypointTick {
    fn push(&mut self, level: &str, player: waypoints::Player) {
        if let Some(conn) = player.conn {
            self.connections.push((player.uuid, conn));
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
        scratch.connections.clear();
        scratch.respawned.clear();
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
                scratch.respawned.push(g.uuid);
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
                &list.players,
                &scratch.connections,
                &scratch.respawned,
                locator_bar,
                &mut scratch.out,
            );
            if list.players.is_empty() {
                host.waypoints.remove(level);
            }
        }
        scratch.levels.retain(|_, list| list.used != 0);
        // A packet lookup must not scan all guests for every waypoint.
        let guests: std::collections::HashMap<_, _> =
            host.hub.guests.iter().map(|g| (g.conn, g)).collect();
        for (conn, packet) in scratch.out.drain(..) {
            if conn == HOST_CONN {
                // The host's own connection: its locator bar.
                if let horizon_protocol::packets::play::Clientbound::TrackedWaypoint(w) = &packet {
                    self.handle_waypoint(w);
                }
            } else if let Some(g) = guests.get(&conn) {
                g.send(&packet);
            }
        }
        host.waypoint_tick = scratch;
    }
}
