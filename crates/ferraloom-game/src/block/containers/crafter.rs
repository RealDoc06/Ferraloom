use super::storage::{RANDOMIZABLE_COMPONENTS, RandomizableContainerBlockEntity};
use crate::entity::player::inventory::{count, is_same_item_same_components, max_stack_size};
use crate::{
    block::entity::{BlockEntity, BlockEntityData, BlockEntityState},
    inventory::Container,
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::{ComponentTypeId, DataComponentMap, ItemStack};
/// Port of CrafterBlockEntity's storage. Redstone owns the trigger and recipe execution.
#[derive(Clone, Debug)]
pub struct CrafterBlockEntity {
    pub parent: RandomizableContainerBlockEntity,
    pub disabled_slots: [bool; 9],
    pub triggered: i32,
    pub crafting_ticks_remaining: i32,
}
impl CrafterBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent: RandomizableContainerBlockEntity::new(parent, 9),
            disabled_slots: [false; 9],
            triggered: 0,
            crafting_ticks_remaining: 0,
        }
    }
    pub fn set_slot_state(&mut self, slot: usize, enabled: bool) {
        if slot < 9 && self.parent.parent.items[slot].is_empty() {
            self.disabled_slots[slot] = !enabled;
            self.parent.parent.set_changed();
        }
    }
    pub fn can_place_item(&self, slot: usize, _stack: &ItemStack) -> bool {
        if self.disabled_slots[slot] {
            return false;
        }
        let current = &self.parent.parent.items[slot];
        let size = count(current);
        if size >= max_stack_size(current) {
            return false;
        }
        current.is_empty()
            || !(slot + 1..9).any(|next| {
                !self.disabled_slots[next] && {
                    let other = &self.parent.parent.items[next];
                    other.is_empty()
                        || count(other) < size && is_same_item_same_components(current, other)
                }
            })
    }
    pub fn analog_output(&self) -> i32 {
        (0..9)
            .filter(|&slot| self.disabled_slots[slot] || !self.parent.parent.items[slot].is_empty())
            .count() as i32
    }
}
pub struct CrafterBehavior;
pub static CRAFTER_ENTITY: CrafterBehavior = CrafterBehavior;
impl BlockEntity for CrafterBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(
        &self,
        _receiver: crate::block::entity::BlockEntityRef,
        _level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        // Redstone's server_tick hook is not released on this base yet.
        Err(crate::level::LevelError::UnsupportedBlockEntity(
            ferraloom_data::Block::CRAFTER,
        ))
    }

    fn pre_remove_side_effects(
        &self,
        entity: crate::block::entity::BlockEntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        super::drops::drop_contents(entity, level)
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["CustomName", "lock", "Items", "LootTable", "LootTableSeed"] {
            tag.remove(key);
        }
    }
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Crafter(crafter) = state else {
            unreachable!()
        };
        crafter.parent.load(tag);
        crafter.crafting_ticks_remaining = tag.get_int_or("crafting_ticks_remaining", 0);
        crafter.triggered = tag.get_int_or("triggered", 0);
        crafter.disabled_slots = [false; 9];
        for &slot in tag.get_int_array("disabled_slots").unwrap_or_default() {
            if (0..9).contains(&slot) && crafter.parent.parent.items[slot as usize].is_empty() {
                crafter.disabled_slots[slot as usize] = true;
            }
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Crafter(crafter) = state else {
            unreachable!()
        };
        let mut tag = crafter.parent.save(true)?;
        tag.insert(
            "crafting_ticks_remaining",
            Tag::Int(crafter.crafting_ticks_remaining),
        );
        tag.insert("triggered", Tag::Int(crafter.triggered));
        tag.insert(
            "disabled_slots",
            Tag::IntArray(
                (0..9)
                    .filter(|&slot| crafter.disabled_slots[slot])
                    .map(|slot| slot as i32)
                    .collect(),
            ),
        );
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Crafter(crafter) = state else {
            unreachable!()
        };
        crafter.parent.apply_components(map);
        RANDOMIZABLE_COMPONENTS
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Crafter(crafter) = state else {
            unreachable!()
        };
        crafter.parent.collect_components(map);
    }
}
