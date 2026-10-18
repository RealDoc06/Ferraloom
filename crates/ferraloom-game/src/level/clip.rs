//! BlockGetter ray queries. All visited positions use Level's required chunk
//! reads; a hit aborts the shared native traversal before the next position.
use super::{Level, LevelError};
use crate::{
    block::{BlockCollisionContext, LevelReaderRef, fluid::FlowingFluid},
    entity::{
        EntityData, EntityRef, collision_context::EntityCollisionView, trace::traverse_blocks,
    },
};
use ferraloom_data::{
    Block, BlockState, EntityType, FluidState,
    registries::fluid,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockHitResult, BlockPos, Direction, Vec3, VoxelShape, shapes};
use std::cell::OnceCell;

/// Port of ClipContext.Block. FALLDAMAGE_RESETTING is the native 26.3 name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipBlock {
    Collider,
    Outline,
    Visual,
    FallDamageResetting,
}

/// Port of ClipContext.Fluid. WATER uses the installed fluid tag membership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipFluid {
    None,
    SourceOnly,
    Any,
    Water,
}

/// Port of ClipBlockStateContext. The predicate tests states, never shapes.
pub struct ClipBlockStateContext<P> {
    pub from: Vec3,
    pub to: Vec3,
    pub is_target_block: P,
}
impl<P> ClipBlockStateContext<P> {
    pub fn new(from: Vec3, to: Vec3, is_target_block: P) -> Self {
        Self {
            from,
            to,
            is_target_block,
        }
    }
}

/// Port of ClipContext, including its captured entity CollisionContext.
pub struct ClipContext<'context> {
    pub from: Vec3,
    pub to: Vec3,
    pub block: ClipBlock,
    pub fluid: ClipFluid,
    pub collision_context: BlockCollisionContext<'context>,
}
impl<'context> ClipContext<'context> {
    pub fn new(
        from: Vec3,
        to: Vec3,
        block: ClipBlock,
        fluid: ClipFluid,
        collision_context: BlockCollisionContext<'context>,
    ) -> Self {
        Self {
            from,
            to,
            block,
            fluid,
            collision_context,
        }
    }

    /// Port of ClipContext#getBlockShape, preserving context and virtual queries.
    pub fn get_block_shape(
        &self,
        state: BlockState,
        level: &Level,
        pos: BlockPos,
    ) -> Result<VoxelShape, LevelError> {
        let reader = LevelReaderRef::Server(level);
        match self.block {
            ClipBlock::Collider => reader.collision_shape(state, pos, &self.collision_context),
            ClipBlock::Outline => reader.shape_with_context(state, pos, &self.collision_context),
            ClipBlock::Visual => reader.visual_shape(state, pos, &self.collision_context),
            ClipBlock::FallDamageResetting => {
                if level
                    .block_tags()?
                    .contains_key(BlockTags::FALL_DAMAGE_RESETTING, state.block())
                {
                    return Ok(shapes::block());
                }
                if let BlockCollisionContext::Entity(context) = &self.collision_context
                    && let Some(entity) = context.entity.entity
                    && level.entity::<EntityData>(entity)?.entity_type == EntityType::PLAYER
                    && (matches!(state.block(), Block::END_GATEWAY | Block::END_PORTAL)
                        || state.block() == Block::NETHER_PORTAL
                            && level.rules().players_nether_portal_default_delay == 0)
                {
                    return Ok(shapes::block());
                }
                Ok(shapes::empty())
            }
        }
    }

    /// Port of ClipContext#getFluidShape, including EmptyFluid's empty shape.
    pub fn get_fluid_shape(
        &self,
        state: FluidState,
        level: &Level,
        pos: BlockPos,
    ) -> Result<VoxelShape, LevelError> {
        let pick = match self.fluid {
            ClipFluid::None => false,
            ClipFluid::SourceOnly => state.source,
            ClipFluid::Any => !state.empty,
            ClipFluid::Water => level
                .fluid_tags()?
                .contains_key(FluidTags::WATER, state.fluid_type),
        };
        if pick {
            level.fluid_shape(state, pos)
        } else {
            Ok(shapes::empty())
        }
    }
}
impl ClipContext<'static> {
    /// Port of ClipContext's Entity constructor; None selects CollisionContext#empty.
    pub fn for_entity(
        from: Vec3,
        to: Vec3,
        block: ClipBlock,
        fluid: ClipFluid,
        entity: Option<EntityRef>,
        level: &Level,
    ) -> Result<Self, LevelError> {
        let context = match entity {
            Some(entity) => {
                BlockCollisionContext::Entity(EntityCollisionView::capture(entity, level, false)?)
            }
            None => BlockCollisionContext::empty(),
        };
        Ok(Self::new(from, to, block, fluid, context))
    }
}

/// Port of FlowingFluid#shapes. The first query fixes the neighbor-dependent
/// height for each fluid state. Region-parallel deviation: this cache is per
/// Level, rather than process-wide; only differing first contexts in separate
/// dimensions can differ from vanilla's sequential singleton cache.
pub(super) struct FluidShapeCache {
    shapes: [OnceCell<VoxelShape>; fluid::ALL.len() * 32],
}
impl Default for FluidShapeCache {
    fn default() -> Self {
        Self {
            shapes: [const { OnceCell::new() }; fluid::ALL.len() * 32],
        }
    }
}
impl Level {
    /// Port of BlockGetter#isBlockInLine. Even a match reports the endpoint's
    /// position and direction, rather than the matched block or a shape hit.
    pub fn is_block_in_line<P: Fn(BlockState) -> bool>(
        &self,
        context: &ClipBlockStateContext<P>,
    ) -> Result<BlockHitResult, LevelError> {
        let matched = traverse_blocks(context.from, context.to, |pos| {
            Ok((context.is_target_block)(self.get_block_state(pos)?))
        })?;
        let delta = context.from - context.to;
        let direction = Direction::get_approximate_nearest(delta.x, delta.y, delta.z);
        let pos = BlockPos::containing(context.to.x, context.to.y, context.to.z);
        Ok(if matched {
            BlockHitResult::new(context.to, direction, pos, false)
        } else {
            BlockHitResult::miss(context.to, direction, pos)
        })
    }

    /// Port of FluidState#getShape / FlowingFluid#getShape / EmptyFluid#getShape.
    pub fn fluid_shape(&self, state: FluidState, pos: BlockPos) -> Result<VoxelShape, LevelError> {
        if state.empty {
            return Ok(shapes::empty());
        }
        let kind =
            FlowingFluid::from_id(state.fluid_type).expect("nonempty native fluid is flowing");
        if state.amount == 9
            && FlowingFluid::from_id(self.get_block_state(pos.above(1))?.data().fluid.fluid_type)
                == Some(kind)
        {
            return Ok(shapes::block());
        }
        let index = usize::from(state.fluid_type.protocol_id()) * 32
            + usize::from(state.amount) * 2
            + usize::from(state.falling);
        let cell = &self.fluid_shapes.shapes[index];
        if let Some(shape) = cell.get() {
            return Ok(shape.clone());
        }
        let height = kind.height(state, self, pos)?;
        let shape = shapes::box_shape(0.0, 0.0, 0.0, 1.0, f64::from(height), 1.0);
        // A reentrant first query may already have initialized this state.
        let _ = cell.set(shape);
        Ok(cell.get().expect("fluid shape was initialized").clone())
    }

    /// Port of BlockGetter#clip. Equal-distance block/fluid hits favor the block.
    pub fn clip(&self, context: &ClipContext<'_>) -> Result<BlockHitResult, LevelError> {
        let mut hit = None;
        traverse_blocks(context.from, context.to, |pos| {
            let state = self.get_block_state(pos)?;
            let fluid = self.get_block_state(pos)?.data().fluid;
            let block_shape = context.get_block_shape(state, self, pos)?;
            let block_hit = self.clip_with_interaction_override(
                context.from,
                context.to,
                pos,
                &block_shape,
                state,
            )?;
            let fluid_hit = context
                .get_fluid_shape(fluid, self, pos)?
                .clip(context.from, context.to, pos)
                .map(BlockHitResult::from);
            let distance = |hit: Option<BlockHitResult>| {
                hit.map_or(f64::MAX, |hit| (context.from - hit.location).length_sqr())
            };
            hit = if distance(block_hit) <= distance(fluid_hit) {
                block_hit
            } else {
                fluid_hit
            };
            Ok(hit.is_some())
        })?;
        Ok(hit.unwrap_or_else(|| {
            let delta = context.from - context.to;
            BlockHitResult::miss(
                context.to,
                Direction::get_approximate_nearest(delta.x, delta.y, delta.z),
                BlockPos::containing(context.to.x, context.to.y, context.to.z),
            )
        }))
    }

    /// Port of BlockGetter#clipWithInteractionOverride. The closer interaction
    /// shape changes only the face, preserving the primary hit location.
    pub fn clip_with_interaction_override(
        &self,
        from: Vec3,
        to: Vec3,
        pos: BlockPos,
        shape: &VoxelShape,
        state: BlockState,
    ) -> Result<Option<BlockHitResult>, LevelError> {
        let Some(hit) = shape.clip(from, to, pos) else {
            return Ok(None);
        };
        let result = BlockHitResult::from(hit);
        let override_hit = LevelReaderRef::Server(self)
            .interaction_shape(state, pos)?
            .clip(from, to, pos);
        Ok(Some(
            if let Some(override_hit) = override_hit
                && (override_hit.location - from).length_sqr()
                    < (result.location - from).length_sqr()
            {
                result.with_direction(override_hit.direction)
            } else {
                result
            },
        ))
    }
}

#[cfg(test)]
mod tests;
