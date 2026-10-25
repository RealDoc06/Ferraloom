//! Port of PowderSnowBlock's contextual collision and bucket pickup.
use super::super::{
    BlockBehavior, BlockCollisionContext, LevelAccessRef, LevelReaderRef, UpdateFlags,
};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockState, EntityType, Item, tags::EntityTypeTags};
use ferraloom_util::{
    BlockPos, VoxelShape,
    shapes::{self, CollisionContext},
};
pub struct PowderSnow;
pub static POWDER_SNOW: PowderSnow = PowderSnow;
/// Port of PowderSnowBlock#canEntityWalkOnPowderSnow, including tagged mobs.
pub fn can_walk_on(entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
    can_walk_on_reader(entity, &LevelReaderRef::Server(level))
}
fn can_walk_on_reader(
    entity: EntityRef,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    let base = reader.entity_state(entity)?.as_entity();
    if reader
        .entity_type_tags()?
        .contains_key(EntityTypeTags::POWDER_SNOW_WALKABLE_MOBS, base.entity_type)
    {
        return Ok(true);
    }
    Ok(reader
        .entity_state(entity)?
        .as_living()
        .is_some_and(|living| {
            living
                .equipment
                .get(ferraloom_protocol::component::EquipmentSlot::Feet)
                .item()
                == Item::LEATHER_BOOTS
        }))
}
impl BlockBehavior for PowderSnow {
    fn collision_shape(
        &self,
        _state: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        if !context.is_placement()
            && let BlockCollisionContext::Entity(entity_context) = context
            && let Some(entity) = entity_context.entity.entity
        {
            let base = reader.entity_state(entity)?.as_entity();
            if base.fall_distance > 2.5 {
                return Ok(shapes::create(0.0, 0.0, 0.0, 1.0, 0.8999999761581421, 1.0));
            }
            if base.entity_type == EntityType::FALLING_BLOCK
                || (can_walk_on_reader(entity, reader)?
                    && context.is_above(&shapes::block(), pos, false)
                    && !context.is_descending())
            {
                return Ok(shapes::block());
            }
        }
        Ok(shapes::empty())
    }
    fn entity_inside_collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        let context = reader.collision_context(Some(entity), false)?;
        let shape = self.collision_shape(state, pos, &context, reader)?;
        Ok(if shape.is_empty() {
            shapes::block()
        } else {
            shape
        })
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        level.set_block(pos, Block::AIR.default_state(), UpdateFlags::ALL_IMMEDIATE)?;
        match level {
            LevelAccessRef::Server(level) => level.emit(LevelEffect::LevelEvent {
                pos,
                id: 2001,
                data: i32::from(state.id()),
            }),
            LevelAccessRef::Worldgen(_) => { /* WorldGenRegion#levelEvent is a no-op. */ }
        }
        Ok(Some(Item::POWDER_SNOW_BUCKET))
    }
}
