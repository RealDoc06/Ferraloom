use super::Level;
use crate::{block::LevelReaderRef, entity::EntityData, level::LevelError};
use ferraloom_data::BlockState;
use ferraloom_util::{
    Axis, BlockPos, Vec3,
    shapes::{self, BooleanOp},
};

impl Level {
    /// Port of Block#pushEntitiesUp. Both no-context shapes are queried before
    /// capturing the vanilla ordered, non-spectator entity list.
    pub fn push_entities_up(
        &mut self,
        old_state: BlockState,
        new_state: BlockState,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError> {
        let reader = LevelReaderRef::Server(self);
        let old_shape = reader.collision_shape_without_context(old_state, pos)?;
        let new_shape = reader.collision_shape_without_context(new_state, pos)?;
        let added = shapes::join_unoptimized(&old_shape, &new_shape, BooleanOp::OnlySecond)
            .move_by(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        if added.is_empty() {
            return Ok(new_state);
        }
        let mut entities = Vec::new();
        for entity in self.entities_in(added.bounds(), |_, _| true) {
            if !self.entity_behavior(entity)?.is_spectator(entity, self)? {
                entities.push(entity);
            }
        }
        for entity in entities {
            let box_above = self
                .entity::<EntityData>(entity)?
                .bounding_box
                .move_by(0.0, 1.0, 0.0);
            let offset = shapes::collide(Axis::Y, box_above, [&added], -1.0);
            entity.teleport_relative(Vec3::new(0.0, 1.0 + offset, 0.0), self)?;
        }
        Ok(new_state)
    }
}
