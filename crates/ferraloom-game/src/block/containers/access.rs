//! Positional container views used by hopper transfer and comparator queries.
//! These values own generation handles, never inventories or long world borrows.
use super::{furnace::AbstractFurnaceBlockEntity, menu, storage::BaseContainerBlockEntity};
use crate::{
    block::entity::{BlockEntityRef, BlockEntityState},
    entity::player::inventory::{component, count, max_stack_size, set_count, split},
    inventory::{Container, ContainerHandle, ContainerSlot, StackOperation, StackValue},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, Item,
    datapack::{TagSet, tags::ItemTags},
};
use ferraloom_protocol::component::{ComponentTypeId, ItemStack};
use ferraloom_util::{BlockPos, Direction};

/// A single Container or ChestBlock's CompoundContainer, in native slot order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerRef {
    Single(BlockEntityRef),
    Double([BlockEntityRef; 2]),
}
impl ContainerRef {
    pub fn with_callback<R>(
        &self,
        level: &mut Level,
        operation: impl FnOnce(&mut Level) -> Result<R, LevelError>,
    ) -> Result<R, LevelError> {
        for (index, &handle) in self.handles().iter().enumerate() {
            if let Err(error) = level.retain_block_entity(handle) {
                for &retained in &self.handles()[..index] {
                    level.release_block_entity(retained);
                }
                return Err(error);
            }
        }
        let result = operation(level);
        for &handle in self.handles() {
            level.release_block_entity(handle);
        }
        result
    }
    pub fn get_item(&self, slot: usize, level: &mut Level) -> Result<ItemStack, LevelError> {
        let (handle, slot) = self.slot(slot, level)?;
        get_item(handle, slot, level)
    }
    pub fn handles(&self) -> &[BlockEntityRef] {
        match self {
            Self::Single(handle) => std::slice::from_ref(handle),
            Self::Double(handles) => handles,
        }
    }
    pub fn size(&self, level: &Level) -> Result<usize, LevelError> {
        self.handles().iter().try_fold(0, |size, &handle| {
            Ok(size
                + level
                    .block_entity::<BaseContainerBlockEntity>(handle)?
                    .size())
        })
    }
    /// Resolve a combined slot for one short canonical borrow. Keep the original
    /// handle across callbacks: replacing the block must not retarget the view.
    pub fn slot(
        &self,
        mut slot: usize,
        level: &Level,
    ) -> Result<(BlockEntityRef, usize), LevelError> {
        for &handle in self.handles() {
            let size = level
                .block_entity::<BaseContainerBlockEntity>(handle)?
                .size();
            if slot < size {
                return Ok((handle, slot));
            }
            slot -= size;
        }
        Err(LevelError::Inventory("container slot out of range"))
    }
}

/// Port of BaseContainerBlockEntity#getItem and RandomizableContainer's lazy
/// override. Crafter overrides getItem with direct storage, without unpacking.
pub fn get_item(
    handle: BlockEntityRef,
    slot: usize,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    if let BlockEntityState::ChiseledBookShelf(entity) = level.block_entity_state(handle)? {
        return entity
            .items
            .get(slot)
            .cloned()
            .ok_or(LevelError::Inventory("bookshelf slot out of range"));
    }
    if !matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::Crafter(_)
    ) {
        super::loot::unpack_loot_table(handle, None, level)?;
    }
    Ok(level
        .block_entity::<BaseContainerBlockEntity>(handle)?
        .get_item(slot))
}

/// Port of container setItem dispatch. Each world callback sees the write before
/// this function returns; Hopper deliberately omits setChanged here.
pub fn set_item(
    handle: BlockEntityRef,
    slot: usize,
    stack: ItemStack,
    level: &mut Level,
) -> Result<(), LevelError> {
    set_item_value(handle, slot, StackValue::New(stack), level)
}
/// Native class setItem with a fresh value or the original mutable receiver.
/// Existing delivery preserves its generation/alias, then runs the same class
/// callbacks as New; it never clones a held stack into the destination.
pub fn set_item_value(
    handle: BlockEntityRef,
    slot: usize,
    value: StackValue,
    level: &mut Level,
) -> Result<(), LevelError> {
    if matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::ChiseledBookShelf(_)
    ) {
        return super::bookshelf::set_item_value(handle, slot, value, level);
    }
    let mut operation = StackOperation::new(level);
    if let StackValue::Existing(reference) = &value {
        operation.pin_existing(*reference).map_err(stack_error)?;
    }
    operation.level.with_block_entity_callback(handle, |level| {
        super::loot::unpack_loot_table(handle, None, level)?;
        if matches!(level.block_entity_state(handle)?, BlockEntityState::Furnace(_)) {
            return super::furnace::set_item_value(handle, slot, value, level);
        }
        let enable_crafter_slot = matches!(level.block_entity_state(handle)?, BlockEntityState::Crafter(crafter) if crafter.disabled_slots[slot] && crafter.parent.parent.items[slot].is_empty());
        if enable_crafter_slot {
            level.block_entity_mut::<super::crafter::CrafterBlockEntity>(handle)?.set_slot_state(slot, true);
            handle.set_changed(level)?;
        }
        let hopper = matches!(level.block_entity_state(handle)?, BlockEntityState::Hopper(_));
        replace_and_limit(handle, slot, value, level)?;
        if !hopper {
            level.block_entity_mut::<BaseContainerBlockEntity>(handle)?.set_changed();
            handle.set_changed(level)?;
        }
        Ok(())
    })
}
fn stack_error(error: crate::inventory::MenuError) -> LevelError {
    match error {
        crate::inventory::MenuError::Level(error) => error,
        _ => LevelError::Inventory("container stack identity"),
    }
}
/// Shared native items.set followed by limitSize. Class-specific notifications
/// and furnace reset logic run in their callers after both mutations complete.
pub(crate) fn replace_and_limit(
    handle: BlockEntityRef,
    slot: usize,
    value: StackValue,
    level: &mut Level,
) -> Result<(), LevelError> {
    let address = ContainerSlot {
        container: ContainerHandle::BlockEntity(handle),
        slot,
    };
    level
        .replace_stack_object(address, value)
        .map_err(stack_error)?;
    let installed = level.stack_ref(address).map_err(stack_error)?;
    let item = level.stack(installed).map_err(stack_error)?;
    let size = count(item).min(
        level
            .block_entity::<BaseContainerBlockEntity>(handle)?
            .max_stack_size()
            .min(max_stack_size(item)),
    );
    set_count(level.stack_mut(installed).map_err(stack_error)?, size);
    Ok(())
}

/// Port of ContainerHelper#removeItem and the receiver's change notification.
pub fn remove_item(
    handle: BlockEntityRef,
    slot: usize,
    amount: i32,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    if matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::ChiseledBookShelf(_)
    ) {
        return Err(LevelError::Inventory(
            "bookshelf removal requires a caller-rooted stack operation",
        ));
    }
    level.with_block_entity_callback(handle, |level| {
        super::loot::unpack_loot_table(handle, None, level)?;
        let hopper = matches!(
            level.block_entity_state(handle)?,
            BlockEntityState::Hopper(_)
        );
        let container = level.block_entity_mut::<BaseContainerBlockEntity>(handle)?;
        let result = if amount <= 0 {
            ItemStack::default()
        } else {
            split(&mut container.items[slot], amount)
        };
        if !hopper && !result.is_empty() {
            container.set_changed();
            handle.set_changed(level)?;
        }
        Ok(result)
    })
}
pub fn remove_item_no_update(
    handle: BlockEntityRef,
    slot: usize,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    if let BlockEntityState::ChiseledBookShelf(entity) = level.block_entity_state_mut(handle)? {
        return entity
            .items
            .get_mut(slot)
            .map(|item| split(item, 1))
            .ok_or(LevelError::Inventory("bookshelf slot out of range"));
    }
    super::loot::unpack_loot_table(handle, None, level)?;
    let container = level.block_entity::<BaseContainerBlockEntity>(handle)?;
    if let Some((generations, item, index)) = container.stack_parts(slot)
        && !item.is_empty()
        && generations.is_retained(generations.current(index).unwrap())
    {
        return Err(LevelError::Inventory(
            "retained removal requires an Existing stack value",
        ));
    }
    Ok(level
        .block_entity_mut::<BaseContainerBlockEntity>(handle)?
        .remove_item_no_update(slot))
}

/// Port of AbstractContainerMenu#getRedstoneSignalFromContainer, invoking each
/// native getItem at its original point so the first read can unpack chest loot.
pub fn analog_output(container: &ContainerRef, level: &mut Level) -> Result<i32, LevelError> {
    container.with_callback(level, |level| analog_output_inner(container, level))
}
fn analog_output_inner(container: &ContainerRef, level: &mut Level) -> Result<i32, LevelError> {
    let size = container.size(level)?;
    let mut fullness = 0.0f32;
    for slot in 0..size {
        let (handle, _) = container.slot(slot, level)?;
        let stack = container.get_item(slot, level)?;
        if !stack.is_empty() {
            let limit = level
                .block_entity::<BaseContainerBlockEntity>(handle)?
                .max_stack_size()
                .min(max_stack_size(&stack));
            fullness += count(&stack) as f32 / limit as f32;
        }
    }
    fullness /= size as f32;
    Ok((fullness * 14.0).floor() as i32 + i32::from(fullness > 0.0))
}

/// Port of HopperBlockEntity#getBlockContainer. Entity selection (including its
/// RNG draw) belongs to getContainerAt's caller. Chest obstruction is ignored.
pub fn block_container_at(
    pos: BlockPos,
    level: &Level,
) -> Result<Option<ContainerRef>, LevelError> {
    let state = level.get_block_state(pos)?;
    if state.block() == Block::COMPOSTER {
        return Err(LevelError::Inventory("ComposterBlock worldly container"));
    }
    if !state.has_block_entity() {
        return Ok(None);
    }
    let Some(handle) = level.block_entity_ref(pos)? else {
        return Ok(None);
    };
    let receiver = level.block_entity_state(handle)?;
    if matches!(
        receiver,
        BlockEntityState::ChiseledBookShelf(_) | BlockEntityState::DecoratedPot(_)
    ) {
        return Err(LevelError::UnsupportedBlockEntity(state.block()));
    }
    if matches!(receiver, BlockEntityState::BrewingStand(_)) {
        return Err(LevelError::Inventory("BrewingStand worldly container"));
    }
    if receiver.container().is_none() {
        return Ok(None);
    }
    if matches!(receiver, BlockEntityState::Chest(_)) {
        let handles = menu::combine_chest(state, pos, true, level)?;
        return Ok(match handles.as_slice() {
            [] => None,
            &[single] => Some(ContainerRef::Single(single)),
            &[first, second] => Some(ContainerRef::Double([first, second])),
            _ => unreachable!(),
        });
    }
    Ok(Some(ContainerRef::Single(handle)))
}

/// Port of WorldlyContainer#getSlotsForFace; ordinary containers expose all slots.
pub fn slots_for_face(state: &BlockEntityState, face: Direction) -> Vec<usize> {
    if matches!(state, BlockEntityState::Furnace(_)) {
        return match face {
            Direction::Up => vec![0],
            Direction::Down => vec![2, 1],
            _ => vec![1],
        };
    }
    (0..state.container().expect("container view").size()).collect()
}

/// Port of Container#canPlaceItem. Callers performing transfers first read the
/// destination through getItem, which unpacks any pending loot before this test.
/// Tag-gated receivers read the canonical Level service at each query; ordinary
/// predicates retain their native pure behavior without requiring item tags.
pub fn can_place_item(
    state: &BlockEntityState,
    slot: usize,
    stack: &ItemStack,
    level: &Level,
) -> Result<bool, LevelError> {
    Ok(match state {
        BlockEntityState::ChiseledBookShelf(shelf) => {
            if !level
                .item_tags()?
                .contains_key(ItemTags::BOOKSHELF_BOOKS, stack.item())
            {
                return Ok(false);
            }
            let current = shelf
                .items
                .get(slot)
                .ok_or(LevelError::Inventory("bookshelf slot out of range"))?;
            current.is_empty() || count(current) < 1.min(max_stack_size(stack))
        }
        BlockEntityState::Furnace(furnace) => furnace.can_place_item(slot, stack),
        BlockEntityState::Crafter(crafter) => crafter.can_place_item(slot, stack),
        _ => true,
    })
}
/// Port of WorldlyContainer#canPlaceItemThroughFace. The hopper must also call
/// canPlaceItem; shulker's ordinary predicate differs from its sided predicate.
pub fn can_place_item_through_face(
    state: &BlockEntityState,
    slot: usize,
    stack: &ItemStack,
    _face: Option<Direction>,
) -> bool {
    match state {
        BlockEntityState::Furnace(furnace) => furnace.can_place_item(slot, stack),
        BlockEntityState::ShulkerBox(_) => !stack.0.as_ref().is_some_and(|stack| {
            stack.count > 0
                && stack
                    .item
                    .data()
                    .block
                    .is_some_and(|block| block.class() == BlockClass::ShulkerBoxBlock)
        }),
        _ => true,
    }
}
/// Port of AbstractFurnaceBlockEntity#canTakeItemThroughFace. The supplied tags
/// are the current datapack snapshot, not the built-in membership set.
pub fn can_take_item_through_face(
    state: &BlockEntityState,
    slot: usize,
    stack: &ItemStack,
    face: Direction,
    tags: &TagSet<Item>,
) -> bool {
    if matches!(state, BlockEntityState::Furnace(_)) && face == Direction::Down && slot == 1 {
        let item = stack.item();
        return tags.contains_key(ItemTags::FURNACE_FUEL_BOTTOM_TAKEABLE, item);
    }
    true
}

impl AbstractFurnaceBlockEntity {
    /// Port of AbstractFurnaceBlockEntity#canPlaceItem.
    pub fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        match slot {
            2 => false,
            1 => {
                component(stack, ComponentTypeId::COOKING_FUEL).is_some()
                    || !stack.is_empty()
                        && stack
                            .0
                            .as_ref()
                            .is_some_and(|stack| stack.item == Item::BUCKET)
                        && (self.parent.items[1].is_empty()
                            || !self.parent.items[1]
                                .0
                                .as_ref()
                                .is_some_and(|stack| stack.item == Item::BUCKET))
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entity::create;
    use ferraloom_data::datapack::{Pack, ResourceManager};
    #[test]
    fn value_setter_preserves_aliases_and_native_class_callbacks() {
        use crate::level::{Region, chunk::LevelChunk};
        use ferraloom_util::ChunkPos;
        use std::sync::Arc;
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut checked = 0;
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("container-set-value\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            let pos = BlockPos::new(3, 3, 3);
            let section = ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            );
            let state = Block::from_name(&format!("minecraft:{}", fields[1]))
                .unwrap()
                .default_state();
            let mut chunk = LevelChunk::new(ChunkPos::new(0, 0), 0..1, section);
            chunk.set_block_state(pos, state);
            level.add_chunk(chunk).unwrap();
            level.set_block_entity(create(pos, state).unwrap()).unwrap();
            let handle = level.block_entity_ref(pos).unwrap().unwrap();
            set_item(
                handle,
                0,
                ItemStack::new(Item::STONE, 10, Default::default()),
                &mut level,
            )
            .unwrap();
            let address = ContainerSlot {
                container: ContainerHandle::BlockEntity(handle),
                slot: 0,
            };
            let mut operation = StackOperation::new(&mut level);
            let held = operation.pin(address).unwrap();
            set_count(operation.level.stack_mut(held).unwrap(), 9);
            let value = if fields[2] == "true" {
                StackValue::Existing(held)
            } else {
                // Dropper's native path explicitly copies its result.
                StackValue::New(operation.level.stack(held).unwrap().clone())
            };
            let before = operation
                .level
                .block_entity::<BaseContainerBlockEntity>(handle)
                .unwrap()
                .times_changed;
            set_item_value(handle, 0, value, operation.level).unwrap();
            let after = operation
                .level
                .block_entity::<BaseContainerBlockEntity>(handle)
                .unwrap()
                .times_changed;
            assert_eq!(
                after.wrapping_sub(before),
                fields[3].parse::<u32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                operation.level.stack_ref(address).unwrap() == held,
                fields[4] == "true",
                "{line}"
            );
            set_count(operation.level.stack_mut(held).unwrap(), 2);
            let current = operation.level.stack_ref(address).unwrap();
            assert_eq!(
                count(operation.level.stack(current).unwrap()),
                fields[5].parse::<i32>().unwrap(),
                "{line}"
            );
            assert_eq!(
                count(operation.level.stack(held).unwrap()),
                fields[6].parse::<i32>().unwrap(),
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 6);
    }
    #[test]
    fn furnace_sided_slots_and_rules_match_native_live_tags() {
        let level = Level::new(
            crate::level::Region::new(&crate::Behaviors::EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        let tags = TagSet::<Item>::load(&resources).unwrap();
        let mut checked = 0;
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("sided-"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut state = create(
                BlockPos::ZERO,
                Block::from_name(fields[1]).unwrap().default_state(),
            )
            .unwrap();
            let face = Direction::VALUES
                .into_iter()
                .find(|face| format!("{face:?}").eq_ignore_ascii_case(fields[2]))
                .unwrap();
            if fields[0] == "sided-slots" {
                assert_eq!(
                    format!("{:?}", slots_for_face(&state, face)),
                    fields[3],
                    "{line}"
                );
            } else {
                if fields[3] == "true" {
                    state.container_mut().unwrap().items[1] =
                        ItemStack::new(Item::BUCKET, 1, Default::default());
                }
                let stack =
                    ItemStack::new(Item::from_name(fields[4]).unwrap(), 1, Default::default());
                let actual = (0..3)
                    .map(|slot| {
                        format!(
                            "{},{},{}",
                            can_place_item(&state, slot, &stack, &level).unwrap(),
                            can_place_item_through_face(&state, slot, &stack, Some(face)),
                            can_take_item_through_face(&state, slot, &stack, face, &tags)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                assert_eq!(actual, fields[5], "{line}");
            }
            checked += 1;
        }
        assert_eq!(checked, 234);
    }
}
