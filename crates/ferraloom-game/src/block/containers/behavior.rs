//! Method-specific registration for the container block family.
use super::{
    menu::{self, ContainerMenuProvider},
    storage::BaseContainerBlockEntity,
};
use crate::{
    block::{
        BlockBehavior, BlockMethod, InteractionResult, LevelAccessRef, LevelReaderRef, UseContext,
        entity::BlockEntityType,
    },
    inventory::MenuKind,
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_protocol::chat::Component;
use ferraloom_util::{BlockPos, Direction, VoxelShape, shapes};

pub struct ContainerBlock;
static CONTAINER_BLOCK: ContainerBlock = ContainerBlock;
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
    if block.class() == BlockClass::ShulkerBoxBlock && method == M::AnalogOutputSignal {
        return Some(&CONTAINER_BLOCK);
    }
    if supported(block)
        && matches!(
            method,
            M::UseWithoutItem
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
    if matches!(
        block.class(),
        BlockClass::ChestBlock | BlockClass::TrappedChestBlock
    ) && method == M::UpdateShape
    {
        return Some(&CONTAINER_BLOCK);
    }
    None
}
impl BlockBehavior for ContainerBlock {
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let refs = if chest(state.block()) {
            menu::combine_chest(state, pos, false, level)?
        } else {
            level.block_entity_ref(pos)?.into_iter().collect()
        };
        if refs.is_empty() {
            return Ok(InteractionResult::SUCCESS);
        }
        let kind = match state.block().block_entity_type().unwrap() {
            BlockEntityType::CHEST | BlockEntityType::TRAPPED_CHEST | BlockEntityType::BARREL => {
                MenuKind::Chest {
                    rows: if refs.len() == 2 { 6 } else { 3 },
                }
            }
            BlockEntityType::FURNACE => MenuKind::Furnace,
            BlockEntityType::SMOKER => MenuKind::Smoker,
            BlockEntityType::BLAST_FURNACE => MenuKind::BlastFurnace,
            BlockEntityType::CRAFTER => MenuKind::Crafter,
            _ => return Err(LevelError::UnsupportedBlockEntity(state.block())),
        };
        let custom = refs.iter().find_map(|&handle| {
            level
                .block_entity::<BaseContainerBlockEntity>(handle)
                .ok()
                .and_then(|entity| entity.custom_name.clone())
        });
        let key = match kind {
            MenuKind::Chest { rows: 6 } => "container.chestDouble",
            MenuKind::Chest { .. } if state.block() == Block::BARREL => "container.barrel",
            MenuKind::Chest { .. } => "container.chest",
            MenuKind::Furnace => "container.furnace",
            MenuKind::Smoker => "container.smoker",
            MenuKind::BlastFurnace => "container.blast_furnace",
            MenuKind::Crafter => "container.crafter",
            _ => unreachable!(),
        };
        let provider = ContainerMenuProvider {
            entities: refs,
            kind,
            title: custom.unwrap_or_else(|| Component::translatable(key, Vec::new())),
        };
        let packets =
            crate::inventory::open_menu(context.player, &provider, level).map_err(|error| {
                match error {
                    crate::inventory::MenuError::Level(error) => error,
                    _ => LevelError::Inventory("container menu runtime"),
                }
            })?;
        level
            .entity_mut::<crate::entity::player::ServerPlayer>(context.player)?
            .menus
            .pending_packets
            .extend(packets);
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
        let container = match refs.as_slice() {
            &[single] => super::access::ContainerRef::Single(single),
            &[first, second] => super::access::ContainerRef::Double([first, second]),
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
        if state.get(p::TYPE) == Some(p::Type::Single) {
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
    fn update_shape(
        &self,
        mut state: BlockState,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.get(p::WATERLOGGED) == Some(true) {
            level.schedule_fluid_tick(_pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
        }
        if neighbor.block() == state.block() && direction.axis().is_horizontal() {
            let neighbor_type = neighbor.get(p::TYPE).unwrap();
            if state.get(p::TYPE) == Some(p::Type::Single)
                && neighbor_type != p::Type::Single
                && state.get(p::FACING) == neighbor.get(p::FACING)
                && menu::connected_direction(neighbor) == direction.opposite()
            {
                state = state
                    .with(
                        p::TYPE,
                        if neighbor_type == p::Type::Left {
                            p::Type::Right
                        } else {
                            p::Type::Left
                        },
                    )
                    .unwrap();
            }
        } else if menu::connected_direction(state) == direction {
            state = state.with(p::TYPE, p::Type::Single).unwrap();
        }
        Ok(state)
    }
}
