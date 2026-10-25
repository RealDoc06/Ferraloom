//! Canonical ServerWaypointManager and its native connection table. One Region
//! currently owns each Level; a future split must retain the per-level service.
use super::{Level, LevelError, effects::LevelEffect};
use crate::entity::{EntityData, EntityRef, LivingEntity, player::ServerPlayer};
use ferraloom_protocol::play::{WaypointIcon, WaypointOperation, clientbound as cb};
use ferraloom_util::collections::{HashSet, JavaHash, LinkedHashMap};

mod connection;
mod icon;
use connection::Connection;
pub use icon::{IconDecodeError, default_icon, load_icon, save_icon};

/// Entity#hashCode uses the network id. Keep the handle as canonical identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WaypointKey {
    entity: EntityRef,
    network: i32,
}
impl JavaHash for WaypointKey {
    fn java_hash_code(&self) -> i32 {
        self.network
    }
}

/// Port of ServerWaypointManager. The pinned Guava 33.6 HashBasedTable uses
/// LinkedHashMap for both rows and cells; the membership sets remain HashSet.
#[derive(Default)]
pub struct ServerWaypointManager {
    waypoints: HashSet<WaypointKey>,
    players: HashSet<WaypointKey>,
    connections: LinkedHashMap<WaypointKey, LinkedHashMap<WaypointKey, Connection>>,
}
impl ServerWaypointManager {
    fn connection(&self, player: WaypointKey, source: WaypointKey) -> Option<&Connection> {
        self.connections.peek(&player)?.peek(&source)
    }
    fn put(&mut self, player: WaypointKey, source: WaypointKey, connection: Connection) {
        if !self.connections.contains_key(&player) {
            // HashBasedTable.Factory(0) -> Maps.newLinkedHashMapWithExpectedSize(0).
            self.connections
                .insert(player, LinkedHashMap::with_capacity(1));
        }
        self.connections
            .get_mut(&player)
            .unwrap()
            .insert(source, connection);
    }
    fn remove(&mut self, player: WaypointKey, source: WaypointKey) -> Option<Connection> {
        let row = self.connections.get_mut(&player)?;
        let connection = row.remove(&source);
        if row.is_empty() {
            self.connections.remove(&player);
        }
        connection
    }
    fn column(&self, source: WaypointKey) -> Vec<(WaypointKey, Connection)> {
        self.connections
            .iter()
            .filter_map(|(player, row)| {
                row.peek(&source)
                    .map(|connection| (*player, connection.clone()))
            })
            .collect()
    }
}
impl Level {
    /// Port of ServerPlayer#restoreFrom's Waypoint.Icon#copyFrom. Respawn's
    /// producer calls this helper at its native copy point when that system lands.
    pub fn copy_waypoint_icon(
        &mut self,
        source: EntityRef,
        destination: EntityRef,
    ) -> Result<(), LevelError> {
        let icon = self
            .entity::<LivingEntity>(source)?
            .locator_bar_icon
            .clone();
        self.entity_mut::<LivingEntity>(destination)?
            .locator_bar_icon = icon;
        Ok(())
    }
    fn waypoint_key(&self, entity: EntityRef) -> Result<WaypointKey, LevelError> {
        Ok(WaypointKey {
            entity,
            network: self.entity::<EntityData>(entity)?.network_id,
        })
    }
    fn waypoint_packet(&mut self, player: WaypointKey, packet: cb::TrackedWaypoint) {
        self.emit(LevelEffect::PlayerPacket {
            recipients: vec![player.entity],
            packet: Box::new(cb::Packet::Waypoint(packet)),
        });
    }
    fn waypoint_create_connection(
        &mut self,
        player: WaypointKey,
        source: WaypointKey,
    ) -> Result<(), LevelError> {
        if player == source || !self.rules().locator_bar {
            return Ok(());
        }
        if let Some(connection) = Connection::make(source.entity, player.entity, self)? {
            let packet = connection.packet(WaypointOperation::Track, self)?;
            // createConnection installs the cell BEFORE connect sends its packet.
            self.region_mut()
                .waypoint_manager
                .put(player, source, connection);
            self.waypoint_packet(player, packet);
        } else if let Some(connection) = self.region_mut().waypoint_manager.remove(player, source) {
            self.waypoint_packet(player, connection.packet(WaypointOperation::Untrack, self)?);
        }
        Ok(())
    }
    fn waypoint_update_connection(
        &mut self,
        player: WaypointKey,
        source: WaypointKey,
        previous: Connection,
    ) -> Result<(), LevelError> {
        if player == source || !self.rules().locator_bar {
            return Ok(());
        }
        if !previous.is_broken(self)? {
            if let Some(next) = previous.update(self)? {
                self.waypoint_packet(player, next.packet(WaypointOperation::Update, self)?);
                self.region_mut().waypoint_manager.put(player, source, next);
            }
        } else if let Some(next) = Connection::make(source.entity, player.entity, self)? {
            // updateConnection's broken-cell replacement connects BEFORE put.
            self.waypoint_packet(player, next.packet(WaypointOperation::Track, self)?);
            self.region_mut().waypoint_manager.put(player, source, next);
        } else {
            self.waypoint_packet(player, previous.packet(WaypointOperation::Untrack, self)?);
            self.region_mut().waypoint_manager.remove(player, source);
        }
        Ok(())
    }
    /// Port of LivingEntity#isTransmittingWaypoint.
    pub fn is_transmitting_waypoint(&self, entity: EntityRef) -> Result<bool, LevelError> {
        if self.entity_state(entity)?.as_living().is_none() {
            return Ok(false);
        }
        Ok(crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::WAYPOINT_TRANSMIT_RANGE,
            self,
        )? > 0.0)
    }
    /// Port of ServerPlayer#isReceivingWaypoints.
    pub fn is_receiving_waypoints(&self, player: EntityRef) -> Result<bool, LevelError> {
        self.entity::<ServerPlayer>(player)?;
        Ok(crate::entity::living::attribute(
            player,
            ferraloom_data::registries::attribute::Id::WAYPOINT_RECEIVE_RANGE,
            self,
        )? > 0.0)
    }
    /// Port of ServerWaypointManager#trackWaypoint. Repeated tracking still
    /// recreates connections; HashSet insertion's return value is not a guard.
    pub fn track_waypoint(&mut self, source: EntityRef) -> Result<(), LevelError> {
        self.entity::<LivingEntity>(source)?;
        let source = self.waypoint_key(source)?;
        self.region_mut().waypoint_manager.waypoints.insert(source);
        let players: Vec<_> = self
            .region()
            .waypoint_manager
            .players
            .iter()
            .copied()
            .collect();
        for player in players {
            self.waypoint_create_connection(player, source)?;
        }
        Ok(())
    }
    /// Port of ServerWaypointManager#updateWaypoint, retaining the immutable
    /// entry snapshot and the subsequently evaluated live Sets.difference view.
    pub fn update_waypoint(&mut self, source: EntityRef) -> Result<(), LevelError> {
        let source = self.waypoint_key(source)?;
        if !self.region().waypoint_manager.waypoints.contains(&source) {
            return Ok(());
        }
        let connected = self.region().waypoint_manager.column(source);
        for (player, previous) in connected {
            self.waypoint_update_connection(player, source, previous)?;
        }
        let potential: Vec<_> = self
            .region()
            .waypoint_manager
            .players
            .iter()
            .copied()
            .filter(|player| {
                self.region()
                    .waypoint_manager
                    .connection(*player, source)
                    .is_none()
            })
            .collect();
        for player in potential {
            self.waypoint_create_connection(player, source)?;
        }
        Ok(())
    }
    /// Port of ServerWaypointManager#untrackWaypoint.
    pub fn untrack_waypoint(&mut self, source: EntityRef) -> Result<(), LevelError> {
        let source = self.waypoint_key(source)?;
        let connections = self.region().waypoint_manager.column(source);
        for (player, connection) in &connections {
            self.waypoint_packet(
                *player,
                connection.packet(WaypointOperation::Untrack, self)?,
            );
        }
        for (player, _) in connections {
            self.region_mut().waypoint_manager.remove(player, source);
        }
        self.region_mut().waypoint_manager.waypoints.remove(&source);
        Ok(())
    }
    /// Port of ServerWaypointManager#addPlayer. Receiving and transmitting are
    /// independent attributes; a receiver may also become a transmitter here.
    pub fn add_waypoint_player(&mut self, player: EntityRef) -> Result<(), LevelError> {
        self.entity::<ServerPlayer>(player)?;
        let key = self.waypoint_key(player)?;
        self.region_mut().waypoint_manager.players.insert(key);
        let waypoints: Vec<_> = self
            .region()
            .waypoint_manager
            .waypoints
            .iter()
            .copied()
            .collect();
        for source in waypoints {
            self.waypoint_create_connection(key, source)?;
        }
        if self.is_transmitting_waypoint(player)? {
            self.track_waypoint(player)?;
        }
        Ok(())
    }
    /// Port of ServerWaypointManager#updatePlayer.
    pub fn update_waypoint_player(&mut self, player: EntityRef) -> Result<(), LevelError> {
        let player = self.waypoint_key(player)?;
        let connected: Vec<_> = self
            .region()
            .waypoint_manager
            .connections
            .peek(&player)
            .map(|row| {
                row.iter()
                    .map(|(source, connection)| (*source, connection.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (source, previous) in connected {
            self.waypoint_update_connection(player, source, previous)?;
        }
        let potential: Vec<_> = self
            .region()
            .waypoint_manager
            .waypoints
            .iter()
            .copied()
            .filter(|source| {
                self.region()
                    .waypoint_manager
                    .connection(player, *source)
                    .is_none()
            })
            .collect();
        for source in potential {
            self.waypoint_create_connection(player, source)?;
        }
        Ok(())
    }
    /// Port of ServerWaypointManager#removePlayer: disconnect this player's row
    /// before untracking its transmitter and removing receiver membership.
    pub fn remove_waypoint_player(&mut self, player: EntityRef) -> Result<(), LevelError> {
        let key = self.waypoint_key(player)?;
        let connected: Vec<_> = self
            .region()
            .waypoint_manager
            .connections
            .peek(&key)
            .map(|row| {
                row.iter()
                    .map(|(source, connection)| (*source, connection.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (source, connection) in connected {
            self.waypoint_packet(key, connection.packet(WaypointOperation::Untrack, self)?);
            self.region_mut().waypoint_manager.remove(key, source);
        }
        self.untrack_waypoint(player)?;
        self.region_mut().waypoint_manager.players.remove(&key);
        Ok(())
    }
    /// Port of ServerWaypointManager#breakAllConnections. Membership remains.
    pub fn break_all_waypoint_connections(&mut self) -> Result<(), LevelError> {
        let connected: Vec<_> = self
            .region()
            .waypoint_manager
            .connections
            .iter()
            .flat_map(|(player, row)| row.values().map(|connection| (*player, connection.clone())))
            .collect();
        for (player, connection) in connected {
            self.waypoint_packet(player, connection.packet(WaypointOperation::Untrack, self)?);
        }
        self.region_mut().waypoint_manager.connections.clear();
        Ok(())
    }
    /// Port of ServerWaypointManager#remakeConnections, including native Track
    /// replacement without an intervening Untrack when the connection is valid.
    pub fn remake_waypoint_connections(&mut self, source: EntityRef) -> Result<(), LevelError> {
        let source = self.waypoint_key(source)?;
        let players: Vec<_> = self
            .region()
            .waypoint_manager
            .players
            .iter()
            .copied()
            .collect();
        for player in players {
            self.waypoint_create_connection(player, source)?;
        }
        Ok(())
    }
}
