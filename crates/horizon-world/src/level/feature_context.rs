//! Cold feature entry: world operations and RNG draws use the same live context.
use super::{
    Sim, feature_random::FeatureRandomSelector, feature_world::LevelFeatureWorld,
    tick_access::TickContext,
};
use crate::{
    fluid::{FluidKind, FluidState},
    heightmap::HeightmapType,
    registry::BlockStateId,
    worldgen::{
        anchor::WorldGenerationContext,
        biome::BiomeId,
        feature::FeatureWorld,
        live::LiveFeatures,
        placement::{LayerBlock, PlacementContext, PlacementQueryError},
    },
};
use horizon_core::{geometry::Direction, position::BlockPos, random::RandomSource};
use std::cell::RefCell;

pub(super) enum Placement {
    Configured,
    Placed,
}

pub(super) fn place<C: TickContext>(
    sim: &mut Sim<'_, C>,
    name: &str,
    pos: BlockPos,
    placement: Placement,
) -> bool {
    let b = sim.b;
    let Some(features) = b.live_features() else {
        return false;
    };
    // This cell contains only the adapter/context handle. Region and tick-state
    // borrows still end inside their private access helpers before callbacks.
    let context = RefCell::new(sim);
    let mut world = LiveFeatureWorld {
        context: &context,
        features,
    };
    let mut random = FeatureRandomSelector::Live(&context);
    match placement {
        Placement::Configured => {
            features.place_configured(name, &mut world, &mut random, [pos.x, pos.y, pos.z])
        }
        Placement::Placed => {
            features.place_placed(name, &mut world, &mut random, [pos.x, pos.y, pos.z])
        }
    }
}

// Both facades stay private to this call boundary. World callbacks receive the
// underlying live level through Sim, never a handle to this control cell.
struct LiveFeatureWorld<'c, 's, 'a, C: TickContext> {
    context: &'c RefCell<&'s mut Sim<'a, C>>,
    features: &'c LiveFeatures,
}
impl<'s, 'a, C: TickContext> LiveFeatureWorld<'_, 's, 'a, C> {
    fn with_world<R>(&self, call: impl FnOnce(&mut LevelFeatureWorld<'_, 'a, C>) -> R) -> R {
        let mut context = self.context.borrow_mut();
        call(&mut LevelFeatureWorld {
            sim: &mut **context,
            features: self.features,
        })
    }
}

impl<C: TickContext> FeatureWorld for LiveFeatureWorld<'_, '_, '_, C> {
    type Error = PlacementQueryError;
    fn block(&mut self, position: [i32; 3]) -> Result<BlockStateId, Self::Error> {
        self.with_world(|world| world.block(position))
    }
    fn set_block(
        &mut self,
        position: [i32; 3],
        state: BlockStateId,
        flags: i32,
    ) -> Result<bool, Self::Error> {
        self.with_world(|world| world.set_block(position, state, flags))
    }
}

impl<C: TickContext> PlacementContext for LiveFeatureWorld<'_, '_, '_, C> {
    fn update_shape_at_face(
        &mut self,
        position: [i32; 3],
        direction: Direction,
        flags: i32,
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.update_shape_at_face(position, direction, flags))
    }
    fn set_section_block(
        &mut self,
        position: [i32; 3],
        state: BlockStateId,
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.set_section_block(position, state))
    }
    fn set_chunk_block(
        &mut self,
        position: [i32; 3],
        state: BlockStateId,
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.set_chunk_block(position, state))
    }
    fn collision_full_block(
        &mut self,
        state: BlockStateId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.collision_full_block(state, _position))
    }
    fn world_seed(&mut self) -> Result<i64, PlacementQueryError> {
        self.with_world(|world| world.world_seed())
    }
    fn state_collision_empty(
        &mut self,
        state: BlockStateId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.state_collision_empty(state, _position))
    }
    fn can_occlude(&mut self, state: BlockStateId) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.can_occlude(state))
    }
    fn store_beehive_occupants(
        &mut self,
        position: [i32; 3],
        ticks_in_hive: &[i32],
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.store_beehive_occupants(position, ticks_in_hive))
    }
    fn set_block_entity_loot_table(
        &mut self,
        _position: [i32; 3],
        _table: &str,
        _random: &mut impl RandomSource,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn block_brightness(&mut self, position: [i32; 3]) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.block_brightness(position))
    }
    fn fluid_state(&mut self, position: [i32; 3]) -> Result<FluidState, PlacementQueryError> {
        self.with_world(|world| world.fluid_state(position))
    }
    fn destroy_block_without_entity(
        &mut self,
        position: [i32; 3],
        drop_resources: bool,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.destroy_block_without_entity(position, drop_resources))
    }
    fn sea_level(&mut self) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.sea_level())
    }
    fn schedule_fluid_tick(
        &mut self,
        position: [i32; 3],
        fluid: FluidKind,
        delay: i32,
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.schedule_fluid_tick(position, fluid, delay))
    }
    fn mark_postprocessing(&mut self, _position: [i32; 3]) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.mark_postprocessing(_position))
    }
    fn water_at(&mut self, position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.water_at(position))
    }
    fn attachment_at(
        &mut self,
        position: [i32; 3],
        direction: Direction,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.attachment_at(position, direction))
    }
    fn next_world_boolean(&mut self) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.next_world_boolean())
    }
    fn next_world_float(&mut self) -> Result<f32, PlacementQueryError> {
        self.with_world(|world| world.next_world_float())
    }
    fn next_world_int(&mut self, bound: i32) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.next_world_int(bound))
    }
    fn schedule_block_tick(
        &mut self,
        position: [i32; 3],
        block: BlockStateId,
        delay: i32,
    ) -> Result<(), PlacementQueryError> {
        self.with_world(|world| world.schedule_block_tick(position, block, delay))
    }
    fn block_survives(
        &mut self,
        state: BlockStateId,
        position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.block_survives(state, position))
    }
    fn state_face_sturdy(
        &mut self,
        state: BlockStateId,
        _position: [i32; 3],
        direction: Direction,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.state_face_sturdy(state, _position, direction))
    }
    fn face_sturdy(
        &mut self,
        position: [i32; 3],
        direction: Direction,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.face_sturdy(position, direction))
    }
    fn center_sturdy(
        &mut self,
        position: [i32; 3],
        direction: Direction,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.center_sturdy(position, direction))
    }
    fn unobstructed_block(&mut self, position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.unobstructed_block(position))
    }
    fn biome_identity(&mut self, position: [i32; 3]) -> Result<BiomeId, PlacementQueryError> {
        self.with_world(|world| world.biome_identity(position))
    }
    fn block_state(&mut self, position: [i32; 3]) -> Result<BlockStateId, PlacementQueryError> {
        self.with_world(|world| world.block_state(position))
    }
    fn block_matches_tag(
        &mut self,
        position: [i32; 3],
        tag: &str,
    ) -> Result<bool, PlacementQueryError> {
        self.with_world(|world| world.block_matches_tag(position, tag))
    }
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.max_y())
    }
    fn layer_block(&mut self, position: [i32; 3]) -> Result<LayerBlock, PlacementQueryError> {
        self.with_world(|world| world.layer_block(position))
    }
    fn generation(&self) -> WorldGenerationContext {
        self.with_world(|world| world.generation())
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.min_y())
    }
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Result<i32, PlacementQueryError> {
        self.with_world(|world| world.height(kind, x, z))
    }
}
