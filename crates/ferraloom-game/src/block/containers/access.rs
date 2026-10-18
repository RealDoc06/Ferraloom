//! Positional container views used by hopper transfer and comparator queries.
//! These values own generation handles, never inventories or long world borrows.
use super::{furnace::AbstractFurnaceBlockEntity, menu, storage::BaseContainerBlockEntity};
use crate::{
    block::entity::{BlockEntityRef, BlockEntityState},
    entity::player::inventory::component,
    inventory::Container,
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
pub fn can_place_item(state: &BlockEntityState, slot: usize, stack: &ItemStack) -> bool {
    match state {
        BlockEntityState::Furnace(furnace) => furnace.can_place_item(slot, stack),
        BlockEntityState::Crafter(crafter) => crafter.can_place_item(slot, stack),
        _ => true,
    }
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
            // Block#byItem is immutable registry data. Restrict the scan to the
            // seventeen ShulkerBoxBlock entries before comparing their item.
            stack.count > 0
                && ferraloom_data::registries::block::ALL.iter().any(|block| {
                    block.class() == BlockClass::ShulkerBoxBlock && block.as_item() == stack.item
                })
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
        let item = if stack.is_empty() {
            Item::AIR
        } else {
            stack.0.as_ref().unwrap().item
        };
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
    fn furnace_sided_slots_and_rules_match_native_live_tags() {
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
                            can_place_item(&state, slot, &stack),
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
