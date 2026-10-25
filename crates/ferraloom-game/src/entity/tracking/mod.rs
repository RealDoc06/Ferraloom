//! ServerEntity movement encoding and per-player entity tracking.
pub mod codec;
mod server_entity;
pub use server_entity::{ServerEntity, TrackingAudience, TrackingError, TrackingPacket};
mod tracked_entity;
pub use tracked_entity::{DirectedTrackingMessage, PlayerTrackingView, TrackedEntity};

impl crate::level::Level {
    /// Port of ServerLevel#broadcastEntityEvent. The runtime tracker must supply
    /// sendToTrackingPlayersAndSelf; this pending unit effect never guesses an audience.
    pub fn broadcast_entity_event(
        &mut self,
        entity: crate::entity::EntityRef,
        _event: i8,
    ) -> Result<(), crate::level::LevelError> {
        let kind = self.entity_state(entity)?.entity_type();
        self.unported_operation(crate::block::UnportedOperation::EntityMethod(
            kind,
            crate::entity::EntityMethod::BroadcastEntityEvent,
        ))
    }
}
