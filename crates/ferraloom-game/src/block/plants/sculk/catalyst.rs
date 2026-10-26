//! Port of SculkCatalystBlockEntity.CatalystListener's dispatch boundary.
use crate::{
    block::entity::BlockEntityRef,
    level::{
        Level, LevelError,
        game_event::{
            DeliveryMode, GameEventContext, GameEventListener, GameEventListenerRef, PositionSource,
        },
    },
};
use ferraloom_data::registries::game_event::Id as GameEvent;
use ferraloom_util::Vec3;

pub(crate) struct Listener;
pub(crate) static LISTENER: Listener = Listener;
fn receiver(listener: GameEventListenerRef) -> BlockEntityRef {
    match listener {
        GameEventListenerRef::BlockEntity(receiver) => receiver,
        #[cfg(test)]
        GameEventListenerRef::Test(_) => unreachable!("catalyst listener"),
    }
}
impl GameEventListener for Listener {
    fn get_listener_source(
        &self,
        listener: GameEventListenerRef,
        _level: &Level,
    ) -> Result<PositionSource, LevelError> {
        Ok(PositionSource::Block(receiver(listener).pos))
    }
    fn get_listener_radius(
        &self,
        _listener: GameEventListenerRef,
        _level: &Level,
    ) -> Result<i32, LevelError> {
        Ok(8)
    }
    fn get_delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::ByDistance
    }
    fn handle_game_event(
        &self,
        _listener: GameEventListenerRef,
        level: &mut Level,
        event: GameEvent,
        context: GameEventContext,
        _origin: Vec3,
    ) -> Result<bool, LevelError> {
        if event == GameEvent::ENTITY_DIE
            && let Some(source) = context.source_entity
            && level.entity_state(source)?.as_living().is_some()
        {
            // Native next calls wasExperienceConsumed, getExperienceReward,
            // shouldDropExperience and skipDropExperience. The entity death/
            // XP lifecycle is not ported; never accept/ignore this event.
            return Err(LevelError::UnsupportedEntity(
                level.entity_state(source)?.entity_type(),
            ));
        }
        Ok(false)
    }
}
