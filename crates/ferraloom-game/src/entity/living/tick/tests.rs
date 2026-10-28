use super::*;
use crate::{
    Behaviors,
    entity::{EntityState, animal::Cow},
};
use ferraloom_data::EntityType;

struct Retiring;
impl Entity for Retiring {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::animal::CowBehavior.trace_entity_refs(state, references);
    }
    fn ai_step(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _: &TickRateManager,
    ) -> Result<(), LevelError> {
        level.remove_entity(entity)?;
        level.finish_entity_management();
        level.entity_mut::<LivingEntity>(entity)?.speed = 0.25;
        Ok(())
    }
    fn tick_head_turn(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        assert_eq!(level.entity::<LivingEntity>(entity)?.speed, 0.25);
        level.entity_mut::<LivingEntity>(entity)?.y_head_rot = 17.0;
        Ok(())
    }
}
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::COW, &Retiring);
#[test]
fn shared_tick_retains_the_receiver_through_removal_then_releases_it() {
    let mut level = crate::entity::tests::level_with_behaviors(true, &BEHAVIORS);
    let id = level.next_entity_ref();
    let mut cow = Cow::default();
    cow.as_living_mut()
        .set_pos(ferraloom_util::Vec3::new(8.5, 64.0, 8.5));
    cow.as_living_mut().tick_count = 1;
    level
        .add_entity(id, EntityState::Cow(Box::new(cow)))
        .unwrap();
    tick(&Retiring, id, &mut level, &TickRateManager::default()).unwrap();
    assert_eq!(level.entity::<LivingEntity>(id).unwrap().y_head_rot, 17.0);
    level.finish_entity_management();
    assert!(
        matches!(level.entity_state(id),Err(LevelError::MissingEntity(missing)) if missing==id)
    );
}
