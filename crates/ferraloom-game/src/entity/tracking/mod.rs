//! ServerEntity movement encoding and per-player entity tracking.
pub mod codec;
mod server_entity;
pub use server_entity::{ServerEntity, TrackingAudience, TrackingError, TrackingPacket};
mod tracked_entity;
#[cfg(test)]
pub use tracked_entity::DirectedTrackingMessage;
pub use tracked_entity::{PlayerTrackingView, TrackedEntity};

impl crate::level::Level {
    /// Port of ServerLevel#broadcastEntityEvent through
    /// ChunkMap#sendToTrackingPlayersAndSelf, with its audience captured now.
    pub fn broadcast_entity_event(
        &mut self,
        entity: crate::entity::EntityRef,
        event: i8,
    ) -> Result<(), crate::level::LevelError> {
        let network = self.entity::<crate::entity::EntityData>(entity)?.network_id;
        self.broadcast_tracked_packet(
            entity,
            TrackingPacket {
                audience: TrackingAudience::PlayersAndSelf,
                packet: ferraloom_protocol::play::clientbound::Packet::EntityEvent(
                    ferraloom_protocol::play::clientbound::EntityEvent {
                        entity_id: network,
                        event_id: event,
                    },
                ),
            },
        )
        .map_err(|error| error.into_level(self))
    }
    /// Port of ServerLevel#broadcastDamageEvent. Dynamic damage-type numbers
    /// resolve through the synchronized registry context, never local tag IDs.
    pub fn broadcast_damage_event(
        &mut self,
        entity: crate::entity::EntityRef,
        source: &crate::entity::damage::DamageSource,
    ) -> Result<(), crate::level::LevelError> {
        use crate::{entity::EntityData, level::LevelError};
        use ferraloom_protocol::{
            codec::VarInt,
            play::{self, clientbound as packet},
        };
        let registry = "minecraft:damage_type"
            .parse()
            .expect("native registry identifier");
        let entry = source
            .type_holder()
            .name()
            .to_string()
            .parse()
            .expect("validated damage identifier");
        let source_type = self
            .context()
            .registry_context()?
            .entry_id(&registry, &entry)
            .ok_or(LevelError::MissingNetworkRegistryEntry { registry, entry })?;
        let network = |entity| {
            self.entity::<EntityData>(entity)
                .map(|state| state.network_id)
        };
        let packet = packet::Packet::DamageEvent(packet::DamageEvent {
            entity_id: VarInt(network(entity)?),
            source_type: play::DamageTypeId(source_type),
            source_cause_id: play::OptionalVarInt(
                source.causing_entity().map(network).transpose()?,
            ),
            source_direct_id: play::OptionalVarInt(
                source.direct_entity().map(network).transpose()?,
            ),
            source_position: source.source_position_raw().map(|position| play::Vec3 {
                x: position.x,
                y: position.y,
                z: position.z,
            }),
        });
        self.broadcast_tracked_packet(
            entity,
            TrackingPacket {
                audience: TrackingAudience::PlayersAndSelf,
                packet,
            },
        )
        .map_err(|error| error.into_level(self))
    }
}

#[cfg(test)]
mod events_tests;
