//! Stateless virtual block dispatch. Mutable state always belongs to the level.
use crate::level::{Level, LevelError, redstone::Orientation};
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_util::{BlockPos, Direction};
pub mod breaking;
mod clone_item;
pub mod containers;
pub mod dispatch;
pub mod drops;
pub mod entity;
pub mod interaction;
pub use interaction::{InteractionResult, UseContext};
mod families;
pub mod flags;
pub mod fluid;
mod foundational;
mod placement;
pub mod plants;
pub mod redstone;
pub use placement::{BlockPlaceContext, DirectionalPlaceContext, PlacementHit, UseOnContext};
pub use plants::multiface_spreader;
mod shapes;
pub use shapes::BlockCollisionContext;
mod unsupported;
mod worldgen;
pub use flags::{UPDATE_LIMIT, UpdateFlags};
pub(crate) use unsupported::UnportedCalls;
pub use unsupported::{BlockMethod, UnportedBehavior, UnportedBehaviorStats, UnportedOperation};
pub use worldgen::GameWorldgenBlockBehavior;

/// Shared LevelReader/LevelAccessor surface. The borrow of the region is
/// independent of the lifetime of its generation chunk neighborhood.
pub enum LevelAccessRef<'call, 'chunks> {
    Server(&'call mut Level),
    Worldgen(&'call mut ferraloom_worldgen::region::WorldGenRegion<'chunks>),
}
pub enum LevelReaderRef<'call, 'chunks> {
    Server(&'call Level),
    Worldgen(&'call ferraloom_worldgen::region::WorldGenRegion<'chunks>),
}
impl LevelReaderRef<'_, '_> {
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        match self {
            Self::Server(level) => level.get_block_state(pos),
            Self::Worldgen(region) => region
                .block([pos.x, pos.y, pos.z])
                .map_err(|_| LevelError::MissingChunk(ferraloom_util::ChunkPos::containing(pos))),
        }
    }
    pub fn fluid_tags(
        &self,
    ) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>, LevelError>
    {
        match self {
            Self::Server(level) => level.fluid_tags(),
            Self::Worldgen(region) => region
                .fluid_tags()
                .map_err(|_| LevelError::MissingFluidTags),
        }
    }
    pub fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Block>, LevelError> {
        match self {
            Self::Server(level) => level.block_tags(),
            Self::Worldgen(region) => region
                .block_tags()
                .map_err(|_| LevelError::MissingBlockTags),
        }
    }
    pub(crate) fn required_behavior(
        &self,
        block: Block,
        method: BlockMethod,
    ) -> Result<&'static dyn BlockBehavior, LevelError> {
        match self {
            Self::Server(level) => level.required_block_behavior(block, method),
            Self::Worldgen(_) => dispatch::registered_behavior(block, method)
                .ok_or(LevelError::UnsupportedBlockMethod(block, method)),
        }
    }
}
impl<'chunks> LevelAccessRef<'_, 'chunks> {
    /// Port of Block#updateFromNeighbourShapes; every result is required.
    pub fn update_from_neighbor_shapes(
        &mut self,
        mut state: BlockState,
        pos: BlockPos,
    ) -> Result<BlockState, LevelError> {
        for direction in crate::level::redstone::UPDATE_SHAPE_ORDER {
            let neighbor_pos = pos.relative(direction, 1);
            let neighbor = self.get_block_state(neighbor_pos)?;
            let behavior = self
                .as_reader()
                .required_behavior(state.block(), BlockMethod::UpdateFromNeighbourShapes)?;
            state = behavior.update_shape(state, pos, direction, neighbor_pos, neighbor, self)?;
        }
        Ok(state)
    }
    pub fn as_reader(&self) -> LevelReaderRef<'_, 'chunks> {
        match self {
            Self::Server(level) => LevelReaderRef::Server(level),
            Self::Worldgen(region) => LevelReaderRef::Worldgen(region),
        }
    }
    pub fn get_block_state(&self, pos: BlockPos) -> Result<BlockState, LevelError> {
        self.as_reader().get_block_state(pos)
    }
    pub fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: UpdateFlags,
    ) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => level.set_block(pos, state, flags, UPDATE_LIMIT),
            Self::Worldgen(region) => region
                .set_block([pos.x, pos.y, pos.z], state, flags.0)
                .map_err(|_| LevelError::WorldgenMutationFailed(pos)),
        }
    }
    pub fn destroy_block(&mut self, pos: BlockPos, drop: bool) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => level.destroy_block(pos, drop, UPDATE_LIMIT),
            Self::Worldgen(region) => region
                .destroy_block([pos.x, pos.y, pos.z], drop)
                .map_err(|_| LevelError::WorldgenMutationFailed(pos)),
        }
    }
    pub fn schedule_block_tick(
        &mut self,
        pos: BlockPos,
        block: Block,
        delay: i32,
    ) -> Result<(), LevelError> {
        match self {
            Self::Server(level) => level.schedule_block_tick_after(pos, block, delay),
            Self::Worldgen(region) => region
                .schedule_block_tick(ferraloom_worldgen::region::SavedTick {
                    position: [pos.x, pos.y, pos.z],
                    kind: block,
                    priority: 0,
                })
                .map_err(|_| LevelError::MissingChunk(ferraloom_util::ChunkPos::containing(pos))),
        }
    }
    pub fn schedule_fluid_tick(
        &mut self,
        pos: BlockPos,
        fluid: ferraloom_data::registries::fluid::Id,
        delay: i32,
    ) -> Result<(), LevelError> {
        match self {
            Self::Server(level) => level.schedule_fluid_tick_after(pos, fluid, delay),
            Self::Worldgen(region) => region
                .schedule_fluid_tick(ferraloom_worldgen::region::SavedTick {
                    position: [pos.x, pos.y, pos.z],
                    kind: fluid,
                    priority: 0,
                })
                .map_err(|_| LevelError::MissingChunk(ferraloom_util::ChunkPos::containing(pos))),
        }
    }
    pub fn fast_lava(&self) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => level.fast_lava(),
            Self::Worldgen(region) => region
                .fast_lava()
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
}

/// Port of BlockBehaviour's virtual methods. Defaults are vanilla base behavior;
/// only classes explicitly registered by dispatch may inherit these defaults.
pub trait BlockBehavior: Sync {
    /// Port of BlockBehaviour#getDestroyProgress, including its hardness guard.
    fn get_destroy_progress(
        &self,
        state: BlockState,
        _pos: BlockPos,
        player: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<f32, LevelError> {
        breaking::get_destroy_progress(state, player, level)
    }
    /// Port of BlockBehaviour#attack's inherited no-op.
    fn attack(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _player: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#destroy's inherited no-op. MovingPistonBlock overrides it.
    fn destroy(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Block#playerDestroy's ABI. The base stats, exhaustion and drops body is
    /// explicitly pending slice C; family overrides cannot inherit a fake no-op.
    fn player_destroy(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _player: crate::entity::EntityRef,
        _block_entity: Option<entity::BlockEntityRef>,
        _destroyed_with: &ferraloom_protocol::component::ItemStack,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::PlayerDestroy,
        ))
    }
    /// Port of BlockBehaviour#getMenuProvider's absent base provider.
    fn get_menu_provider(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &Level,
    ) -> Result<Option<Box<dyn crate::inventory::MenuProvider>>, LevelError> {
        Ok(None)
    }
    /// Port of BlockBehaviour#getDrops. Subclass ownership is dispatched separately.
    fn get_drops(
        &self,
        state: BlockState,
        context: drops::BlockLootContext,
        level: &mut Level,
    ) -> Result<Vec<ferraloom_protocol::component::ItemStack>, LevelError> {
        drops::get_base_drops(state, context, level)
    }
    /// Port of BlockBehaviour#spawnAfterBreak's base no-op.
    fn spawn_after_break(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _tool: &ferraloom_protocol::component::ItemStack,
        _drop_experience: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#setPlacedBy's inherited no-op.
    fn set_placed_by(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _by: Option<crate::entity::EntityRef>,
        _stack: &ferraloom_protocol::component::ItemStack,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#spawnDestroyByEntityParticles and ServerLevel#levelEvent.
    fn spawn_destroy_by_entity_particles(
        &self,
        state: BlockState,
        pos: BlockPos,
        source: Option<crate::entity::EntityRef>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.level_event(source, 2001, pos, i32::from(state.id()))
    }
    /// Port of Block#spawnDestroyParticles. The breaker is intentionally absent;
    /// subclasses can override this separately from the entity-specific hook.
    fn spawn_destroy_particles(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .required_block_behavior(state.block(), BlockMethod::SpawnDestroyByEntityParticles)?
            .spawn_destroy_by_entity_particles(state, pos, None, level)
    }
    /// Port of Block#playerWillDestroy. The guarded-piglin AI side effect
    /// remains a required unported call until PiglinAi is implemented.
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        level
            .required_block_behavior(state.block(), BlockMethod::SpawnDestroyByEntityParticles)?
            .spawn_destroy_by_entity_particles(state, pos, Some(player), level)?;
        if level.block_tags()?.contains_key(
            ferraloom_data::tags::BlockTags::GUARDED_BY_PIGLINS,
            state.block(),
        ) {
            return Err(LevelError::UnsupportedEntity(
                ferraloom_data::EntityType::PIGLIN,
            ));
        }
        level.game_event(
            ferraloom_data::registries::game_event::Id::BLOCK_DESTROY,
            ferraloom_util::Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            Some(player),
            Some(state),
        );
        Ok(state)
    }
    /// Port of Block#getStateForPlacement's inherited implementation.
    fn get_state_for_placement(
        &self,
        block: Block,
        _context: &BlockPlaceContext<'_>,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        Ok(Some(block.default_state()))
    }
    /// Port of BlockBehaviour#isSignalSource.
    fn is_signal_source(&self, _state: BlockState) -> bool {
        false
    }
    /// Port of BlockBehaviour#hasAnalogOutputSignal.
    fn has_analog_output_signal(&self, _state: BlockState) -> bool {
        false
    }
    /// Port of BlockBehaviour#ownSignal.
    fn own_signal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(0)
    }
    /// Port of BlockBehaviour#getSignal. Re-dispatch ownSignal on the concrete
    /// block: inherited getSignal still observes a subclass's ownSignal.
    fn signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        level
            .required_behavior(state.block(), BlockMethod::OwnSignal)?
            .own_signal(state, pos, level)
    }
    /// Port of BlockBehaviour#getDirectSignal.
    fn direct_signal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _direction: Direction,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<i32, LevelError> {
        Ok(0)
    }
    /// Port of BlockBehaviour#shouldRedstoneWireConnectTo.
    fn should_redstone_wire_connect_to(
        &self,
        state: BlockState,
        _pos: BlockPos,
        direction: Option<Direction>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let source = level
            .required_behavior(state.block(), BlockMethod::IsSignalSource)?
            .is_signal_source(state);
        Ok(source && direction.is_some())
    }
    /// Port of BlockBehaviour#getAnalogOutputSignal.
    fn analog_output_signal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _direction: Direction,
        _level: &mut Level,
    ) -> Result<i32, LevelError> {
        Ok(0)
    }
    /// Port of BlockBehaviour#useWithoutItem's base pass-through.
    fn use_without_item(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _context: &UseContext,
        _level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        Ok(InteractionResult::Pass)
    }
    /// Port of BlockBehaviour#useItemOn's base empty-hand fallback.
    fn use_item_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _context: &UseContext,
        _level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        Ok(InteractionResult::TryWithEmptyHand)
    }
    /// Port of BlockBehaviour#canBeReplaced(BlockState, BlockPlaceContext).
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(state.data().replaceable
            && (context.item_in_hand().is_empty()
                || context
                    .item_in_hand()
                    .0
                    .as_ref()
                    .is_none_or(|stack| stack.item != state.block().as_item())))
    }
    /// Port of BlockBehaviour#getCloneItemStack's base item constructor. Only
    /// inherited implementations select this default through method dispatch.
    fn get_clone_item_stack(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _include_data: bool,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_protocol::component::ItemStack, LevelError> {
        Ok(ferraloom_protocol::component::ItemStack::new(
            state.block().as_item(),
            1,
            ferraloom_protocol::component::DataComponentPatch::default(),
        ))
    }
    /// Port of BlockBehaviour#getEntityInsideCollisionShape's base full cube.
    fn entity_inside_collision_shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::block())
    }
    /// Port of BlockBehaviour#getCollisionShape for registered base families.
    /// Dynamic overrides must implement this method; no extracted baseline is
    /// treated as a live contextual answer.
    fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _context: &BlockCollisionContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        if state.block().has_collision() {
            level.shape(state, pos)
        } else {
            Ok(ferraloom_util::shapes::empty())
        }
    }
    /// Port of BlockBehaviour#getShape's base full cube. Overriding families
    /// are dispatched separately, so this is never a baseline guess.
    fn shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::block())
    }
    fn is_collision_shape_full_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(ferraloom_util::shapes::is_shape_full_block(
            &level.collision_shape_without_context(state, pos)?,
        ))
    }
    fn support_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        level.collision_shape(state, pos, &BlockCollisionContext::empty())
    }
    /// Port of Block#handlePrecipitation's inherited no-op.
    fn handle_precipitation(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _precipitation: ferraloom_worldgen::biome_manager::Precipitation,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn on_place(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn affect_neighbors_after_removal(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn neighbor_changed(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        Ok(state)
    }
    fn update_indirect_neighbor_shapes(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _flags: UpdateFlags,
        _limit: i32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn can_survive(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn tick(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn random_tick(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of BlockState#entityInside, called with the state captured by
    /// Entity#checkInsideBlocks. Entity data stays canonical in the level.
    fn entity_inside(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#fallOn, preserving the f32 reduction before multiplying
    /// fall distance in f64 and dispatching to the canonical entity behavior.
    fn fall_on(
        &self,
        state: BlockState,
        _pos: BlockPos,
        entity: crate::entity::EntityRef,
        fall_distance: f64,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let reduced =
            fall_distance * f64::from(1.0_f32 - state.block().data().fall_distance_reduction);
        let behavior = level.entity_behavior(entity)?;
        behavior.cause_fall_damage(entity, level, reduced, 1.0)?;
        Ok(())
    }
    /// Port of Block#stepOn's base no-op.
    fn step_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Block#bounceOn's base no-op.
    fn bounce_on(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _entity: crate::entity::EntityRef,
        _fall_distance: f64,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn trigger_event(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _a: i32,
        _b: i32,
        _level: &mut Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn place_liquid(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _fluid: FluidState,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn can_place_liquid(
        &self,
        _state: BlockState,
        _fluid: ferraloom_data::registries::fluid::Id,
    ) -> bool {
        false
    }
    fn pickup_block(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<ferraloom_data::Item>, LevelError> {
        Ok(None)
    }
}

/// Port of BlockBehaviour's base implementation, including AirBlock's unchanged
/// simulation methods. Static physical state facts remain in ferraloom-data.
pub struct BaseBlock;
impl BlockBehavior for BaseBlock {}
pub static BASE_BLOCK: BaseBlock = BaseBlock;

/// Port of BaseFireBlock#spawnDestroyByEntityParticles's empty override.
struct NoDestroyParticles;
impl BlockBehavior for NoDestroyParticles {
    fn spawn_destroy_by_entity_particles(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _source: Option<crate::entity::EntityRef>,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
}
static NO_DESTROY_PARTICLES: NoDestroyParticles = NoDestroyParticles;

/// Port of StrawBedBlock#spawnDestroyParticles's distinct level event.
struct StrawBedParticles;
impl BlockBehavior for StrawBedParticles {
    fn spawn_destroy_particles(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.level_event(None, 2014, pos, i32::from(state.id()))
    }
}
static STRAW_BED_PARTICLES: StrawBedParticles = StrawBedParticles;
