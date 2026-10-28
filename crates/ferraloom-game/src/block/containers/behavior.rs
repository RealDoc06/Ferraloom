//! Method-specific registration for the container block family.
use super::menu;
use crate::{
    block::{
        BlockBehavior, BlockMethod, InteractionResult, LevelAccessRef, LevelReaderRef, UseContext,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, Direction, VoxelShape, shapes};

pub struct ContainerBlock;
static CONTAINER_BLOCK: ContainerBlock = ContainerBlock;
#[cfg(test)]
mod chest_tests;

fn chest(block: Block) -> bool {
    matches!(
        block.class(),
        BlockClass::ChestBlock
            | BlockClass::TrappedChestBlock
            | BlockClass::CopperChestBlock
            | BlockClass::WeatheringCopperChestBlock
    )
}
fn supported(block: Block) -> bool {
    chest(block)
        || matches!(
            block.class(),
            BlockClass::BarrelBlock
                | BlockClass::FurnaceBlock
                | BlockClass::SmokerBlock
                | BlockClass::BlastFurnaceBlock
                | BlockClass::CrafterBlock
        )
}
pub(crate) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    use BlockMethod as M;
    if method == M::TriggerEvent && block.class() == BlockClass::EndGatewayBlock {
        return Some(&CONTAINER_BLOCK);
    }
    if block.class() == BlockClass::ChiseledBookShelfBlock
        && matches!(
            method,
            M::UseItemOn
                | M::UseWithoutItem
                | M::AnalogOutputSignal
                | M::AffectNeighborsAfterRemoval
        )
    {
        return Some(&super::bookshelf::BOOKSHELF_BLOCK);
    }
    if method == M::IsPathfindable {
        return super::pathfinding::behavior(block);
    }
    if let Some(behavior) = super::post_placement::ported_behavior(block, method) {
        return Some(behavior);
    }
    if super::placement::supports(block, method) {
        return Some(&super::placement::CONTAINER_PLACEMENT);
    }
    if matches!(
        block.block_entity_type(),
        Some(
            crate::block::entity::BlockEntityType::SIGN
                | crate::block::entity::BlockEntityType::HANGING_SIGN
        )
    ) && matches!(method, M::UseWithoutItem | M::UseItemOn)
    {
        return Some(&super::sign_block::SIGN_BLOCK);
    }
    if method == M::GetMenuProvider
        && matches!(
            block.block_entity_type(),
            Some(
                crate::block::entity::BlockEntityType::SIGN
                    | crate::block::entity::BlockEntityType::HANGING_SIGN
                    | crate::block::entity::BlockEntityType::END_PORTAL
                    | crate::block::entity::BlockEntityType::DISPENSER
                    | crate::block::entity::BlockEntityType::DROPPER
                    | crate::block::entity::BlockEntityType::HOPPER
            )
        )
    {
        return Some(&CONTAINER_BLOCK);
    }
    if block.class() == BlockClass::ShulkerBoxBlock
        && matches!(
            method,
            M::AnalogOutputSignal | M::GetMenuProvider | M::GetDrops
        )
    {
        return Some(&CONTAINER_BLOCK);
    }
    if supported(block)
        && matches!(
            method,
            M::UseWithoutItem
                | M::GetMenuProvider
                | M::TriggerEvent
                | M::AffectNeighborsAfterRemoval
                | M::AnalogOutputSignal
        )
    {
        return Some(&CONTAINER_BLOCK);
    }
    if chest(block) && method == M::OutlineShape {
        return Some(&CONTAINER_BLOCK);
    }
    if chest(block) && matches!(method, M::UpdateShape | M::UpdateFromNeighbourShapes) {
        return Some(&CONTAINER_BLOCK);
    }
    None
}
impl BlockBehavior for ContainerBlock {
    fn get_drops(
        &self,
        state: BlockState,
        context: crate::block::drops::BlockLootContext,
        level: &mut Level,
    ) -> Result<Vec<ferraloom_protocol::component::ItemStack>, LevelError> {
        crate::block::drops::get_shulker_drops(state, context, level)
    }
    fn get_menu_provider(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &Level,
    ) -> Result<Option<Box<dyn crate::inventory::MenuProvider>>, LevelError> {
        Ok(menu::provider(state, pos, level)?
            .map(|provider| Box::new(provider) as Box<dyn crate::inventory::MenuProvider>))
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let Some(provider) = menu::provider(state, pos, level)? else {
            return Ok(InteractionResult::SUCCESS);
        };
        crate::inventory::open_menu_live(context.player, &provider, level).map_err(|error| {
            match error {
                crate::inventory::MenuError::Level(error) => error,
                _ => LevelError::Inventory("container menu runtime"),
            }
        })?;
        let stat = match state.block().class() {
            BlockClass::TrappedChestBlock => Some(crate::stats::custom::Id::TRIGGER_TRAPPED_CHEST),
            BlockClass::ChestBlock
            | BlockClass::CopperChestBlock
            | BlockClass::WeatheringCopperChestBlock => Some(crate::stats::custom::Id::OPEN_CHEST),
            BlockClass::BarrelBlock => Some(crate::stats::custom::Id::OPEN_BARREL),
            BlockClass::FurnaceBlock => Some(crate::stats::custom::Id::INTERACT_WITH_FURNACE),
            BlockClass::SmokerBlock => Some(crate::stats::custom::Id::INTERACT_WITH_SMOKER),
            BlockClass::BlastFurnaceBlock => {
                Some(crate::stats::custom::Id::INTERACT_WITH_BLAST_FURNACE)
            }
            _ => None,
        };
        if let Some(stat) = stat {
            crate::entity::player::award_stat(
                context.player,
                crate::stats::Stat::Custom(stat),
                1,
                level,
            )?;
        }
        if chest(state.block()) || state.block().class() == BlockClass::BarrelBlock {
            crate::entity::monster::piglin::ai::anger_nearby_piglins(level, context.player, true)?;
        }
        Ok(InteractionResult::SUCCESS)
    }
    fn trigger_event(
        &self,
        _state: BlockState,
        pos: BlockPos,
        a: i32,
        b: i32,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(false);
        };
        let behavior = level.block_entity_state(handle)?.behavior();
        Ok(behavior.trigger_event(level.block_entity_state_mut(handle)?, a, b))
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.update_neighbour_for_output_signal(pos, state.block())
    }
    fn analog_output_signal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &mut Level,
    ) -> Result<i32, LevelError> {
        let refs = if chest(state.block()) {
            menu::combine_chest(state, pos, false, level)?
        } else {
            level.block_entity_ref(pos)?.into_iter().collect()
        };
        if refs.is_empty() {
            return Ok(0);
        }
        if let crate::block::entity::BlockEntityState::Crafter(entity) =
            level.block_entity_state(refs[0])?
        {
            return Ok(entity.analog_output());
        }
        let container = match *refs.as_slice() {
            [single] => super::access::ContainerRef::Single(single),
            [first, second] => super::access::ContainerRef::Double([first, second]),
            _ => unreachable!(),
        };
        super::access::analog_output(&container, level)
    }
    fn shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        if state.get(p::TYPE_) == Some(p::Type::Single) {
            return Ok(shapes::create(
                1.0 / 16.0,
                0.0,
                1.0 / 16.0,
                15.0 / 16.0,
                14.0 / 16.0,
                15.0 / 16.0,
            ));
        }
        let (x0, z0, x1, z1) = match menu::connected_direction(state) {
            Direction::North => (1.0, 0.0, 15.0, 15.0),
            Direction::South => (1.0, 1.0, 15.0, 16.0),
            Direction::West => (0.0, 1.0, 15.0, 15.0),
            Direction::East => (1.0, 1.0, 16.0, 15.0),
            _ => unreachable!(),
        };
        Ok(shapes::create(
            x0 / 16.0,
            0.0,
            z0 / 16.0,
            x1 / 16.0,
            14.0 / 16.0,
            z1 / 16.0,
        ))
    }
    /// Port of ChestBlock#updateShape and CopperChestBlock#updateShape. Water
    /// schedules before partner checks; copper then adopts a connected variant.
    fn update_shape(
        &self,
        mut state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.get(p::WATERLOGGED) == Some(true) {
            // WaterFluid#getTickDelay is five in every dimension.
            level.schedule_fluid_tick(pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
        }
        let connects =
            super::placement::chest_can_connect_to(state.block(), neighbor, &level.as_reader())?;
        if connects && direction.axis().is_horizontal() {
            let neighbor_type = neighbor.get(p::TYPE_).unwrap();
            if state.get(p::TYPE_) == Some(p::Type::Single)
                && neighbor_type != p::Type::Single
                && state.get(p::FACING) == neighbor.get(p::FACING)
                && menu::connected_direction(neighbor) == direction.opposite()
            {
                state = state
                    .with(
                        p::TYPE_,
                        if neighbor_type == p::Type::Left {
                            p::Type::Right
                        } else {
                            p::Type::Left
                        },
                    )
                    .unwrap();
            }
        } else if menu::connected_direction(state) == direction {
            state = state.with(p::TYPE_, p::Type::Single).unwrap();
        }
        if state.block().is_instance_of(BlockClass::CopperChestBlock)
            && connects
            && state.get(p::TYPE_) != Some(p::Type::Single)
            && menu::connected_direction(state) == direction
        {
            state = neighbor.block().default_state().with_properties_of(state);
        }
        Ok(state)
    }
}
