//! Port of ChunkMap.TrackedEntity's audience and movement gates.
use super::{ServerEntity, TrackingError, codec::UpdateInterval};
#[cfg(test)]
use super::{TrackingAudience, TrackingPacket};
#[cfg(test)]
use crate::entity::EntityState;
use crate::{
    entity::{EntityData, EntityRef, player::ServerPlayer},
    level::Level,
};
#[cfg(test)]
use ferraloom_protocol::play::{ClientboundMessage, clientbound::Packet};
use ferraloom_util::SectionPos;

/// Captured for this invocation from live server settings and this connection's
/// chunk tracking. It is never retained as another settings or ticket store.
#[derive(Clone, Copy, Debug)]
pub struct PlayerTrackingView {
    pub player: EntityRef,
    pub server_view_distance: i32,
    pub broadcast_range_percentage: i32,
    pub chunk_tracked: bool,
}
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub struct DirectedTrackingMessage {
    pub player: EntityRef,
    pub message: ClientboundMessage,
}

pub struct TrackedEntity {
    pub server_entity: ServerEntity,
    range: i32,
    pub(crate) last_section_pos: SectionPos,
    // Java's connection identity hashes vary across runs. A unique insertion
    // sequence gives the reference-set order Ferraloom intentionally uses.
    pub(crate) seen_by: Vec<EntityRef>,
}
impl TrackedEntity {
    pub fn new(entity: EntityRef, level: &Level) -> Result<Self, TrackingError> {
        let state = level.entity_state(entity)?;
        Ok(Self {
            server_entity: ServerEntity::new(
                entity,
                level,
                UpdateInterval::for_entity_type(state.entity_type()),
                state.entity_type().data().track_deltas,
            )?,
            range: (state.entity_type().data().tracking_range as i32).wrapping_mul(16),
            last_section_pos: SectionPos::from_position(state.as_entity().position),
            seen_by: Vec::new(),
        })
    }
    pub fn seen_by(&self) -> &[EntityRef] {
        &self.seen_by
    }
    fn effective_range(&self, level: &Level, percentage: i32) -> Result<i32, TrackingError> {
        let mut range = self.range;
        let mut passengers = level
            .entity::<EntityData>(self.server_entity.entity)?
            .passengers
            .clone();
        passengers.reverse();
        while let Some(passenger) = passengers.pop() {
            let state = level.entity_state(passenger)?;
            range = range.max((state.entity_type().data().tracking_range as i32).wrapping_mul(16));
            passengers.extend(state.as_entity().passengers.iter().rev().copied());
        }
        Ok(percentage.wrapping_mul(range) / 100)
    }
    /// The shared ChunkMap.TrackedEntity#updatePlayer eligibility test.
    pub(crate) fn visible_to(
        &self,
        level: &Level,
        view: PlayerTrackingView,
    ) -> Result<bool, TrackingError> {
        let entity = self.server_entity.entity;
        if view.player == entity {
            return Ok(false);
        }
        let player = level.entity::<ServerPlayer>(view.player)?;
        let entity_position = level.entity::<EntityData>(entity)?.position;
        let delta = player.position - entity_position;
        let view_distance = player.view_distance.clamp(2, view.server_view_distance);
        let range = self
            .effective_range(level, view.broadcast_range_percentage)?
            .min(view_distance.wrapping_mul(16));
        let range = f64::from(range);
        let visible = delta.x * delta.x + delta.z * delta.z <= range * range
            && level
                .entity_behavior(entity)?
                .broadcast_to_player(entity, view.player, level)?
            && view.chunk_tracked;
        Ok(visible)
    }
}

// Pure value adapter for the established native unit vectors. Production world
// callbacks use Level's canonical tracker entry and ordered packet effects.
#[cfg(test)]
impl TrackedEntity {
    /// Native horizontal-circle boundary is inclusive; vertical separation is
    /// deliberately absent. The chunk-tracked guard remains independent.
    pub fn update_player(
        &mut self,
        level: &mut Level,
        view: PlayerTrackingView,
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let entity = self.server_entity.entity;
        let visible = self.visible_to(level, view)?;
        if visible {
            if self.seen_by.contains(&view.player) {
                return Ok(Vec::new());
            }
            self.seen_by.push(view.player);
            let packets = self.server_entity.pairing_data(level)?;
            let output = vec![DirectedTrackingMessage {
                player: view.player,
                message: ClientboundMessage::Bundle(packets),
            }];
            level
                .entity_behavior(entity)?
                .start_seen_by_player(entity, view.player, level)?;
            Ok(output)
        } else {
            self.remove_player(level, view.player)
        }
    }
    pub fn update_players(
        &mut self,
        level: &mut Level,
        views: &[PlayerTrackingView],
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let mut output = Vec::new();
        for &view in views {
            output.extend(self.update_player(level, view)?);
        }
        Ok(output)
    }
    pub fn remove_player(
        &mut self,
        level: &mut Level,
        player: EntityRef,
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let Some(index) = self.seen_by.iter().position(|&seen| seen == player) else {
            return Ok(Vec::new());
        };
        self.seen_by.remove(index);
        let entity = self.server_entity.entity;
        level
            .entity_behavior(entity)?
            .stop_seen_by_player(entity, player, level)?;
        Ok(vec![DirectedTrackingMessage {
            player,
            message: ClientboundMessage::Packet(Box::new(
                self.server_entity.removal_packet(level)?,
            )),
        }])
    }
    pub fn broadcast_removed(
        &self,
        level: &mut Level,
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let mut output = Vec::new();
        for &player in &self.seen_by {
            level
                .entity_behavior(self.server_entity.entity)?
                .stop_seen_by_player(self.server_entity.entity, player, level)?;
            output.push(DirectedTrackingMessage {
                player,
                message: ClientboundMessage::Packet(Box::new(
                    self.server_entity.removal_packet(level)?,
                )),
            });
        }
        Ok(output)
    }
    pub fn broadcast(
        &self,
        level: &Level,
        update: TrackingPacket,
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let mut output: Vec<_> = self
            .seen_by
            .iter()
            .map(|&player| DirectedTrackingMessage {
                player,
                message: ClientboundMessage::Packet(Box::new(update.packet.clone())),
            })
            .collect();
        if update.audience == TrackingAudience::PlayersAndSelf
            && matches!(
                level.entity_state(self.server_entity.entity)?,
                EntityState::Player(_)
            )
        {
            output.push(DirectedTrackingMessage {
                player: self.server_entity.entity,
                message: ClientboundMessage::Packet(Box::new(update.packet)),
            });
        }
        Ok(output)
    }
    /// ChunkMap invokes this before the entity phase. Frozen simulation does not
    /// disable tracker updates, and dirty metadata alone does not open this gate.
    pub fn tick(
        &mut self,
        level: &mut Level,
        all_players: &[PlayerTrackingView],
    ) -> Result<Vec<DirectedTrackingMessage>, TrackingError> {
        let base = level.entity::<EntityData>(self.server_entity.entity)?;
        let section = SectionPos::from_position(base.position);
        let changed = section != self.last_section_pos;
        let mut output = Vec::new();
        if changed {
            output.extend(self.update_players(level, all_players)?);
            self.last_section_pos = section;
        }
        let needs_sync = level
            .entity::<EntityData>(self.server_entity.entity)?
            .needs_sync;
        if changed || needs_sync || level.is_entity_ticking_chunk(section.chunk()) {
            for packet in self.server_entity.send_changes(level)? {
                output.extend(self.broadcast(level, packet)?);
            }
        }
        Ok(output)
    }
    pub fn send_to_tracking_players(&self, packet: Packet) -> Vec<DirectedTrackingMessage> {
        self.seen_by
            .iter()
            .map(|&player| DirectedTrackingMessage {
                player,
                message: ClientboundMessage::Packet(Box::new(packet.clone())),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
