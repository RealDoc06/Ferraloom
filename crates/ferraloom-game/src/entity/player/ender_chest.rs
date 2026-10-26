//! PlayerEnderChestContainer's canonical 27 slots and transient active chest.
use super::ServerPlayer;
use crate::{
    block::entity::{BlockEntityRef, ender_chest::EnderChestBlockEntity},
    entity::EntityRef,
    inventory::{Container, SimpleContainer},
    level::{Level, LevelError},
};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::component::{ItemStack, Persistent};

/// Port of PlayerEnderChestContainer. Stack identity belongs to SimpleContainer;
/// activeChest is retained once until clear/replacement or player retirement.
#[derive(Debug)]
pub struct PlayerEnderChestContainer {
    contents: SimpleContainer,
    active_chest: Option<BlockEntityRef>,
}
impl Default for PlayerEnderChestContainer {
    fn default() -> Self {
        Self {
            contents: SimpleContainer::new(27),
            active_chest: None,
        }
    }
}
impl PlayerEnderChestContainer {
    pub fn contents(&self) -> &SimpleContainer {
        &self.contents
    }
    pub fn contents_mut(&mut self) -> &mut SimpleContainer {
        &mut self.contents
    }
    pub fn active_chest(&self) -> Option<BlockEntityRef> {
        self.active_chest
    }
    pub fn is_active_chest(&self, chest: BlockEntityRef) -> bool {
        self.active_chest == Some(chest)
    }

    /// Port of PlayerEnderChestContainer#fromSlots. ItemStackWithSlot's optional
    /// Slot codec retains zero as its partial value for a malformed field too.
    pub(crate) fn from_slots(&mut self, entries: &ListTag) {
        for slot in 0..self.contents.size() {
            self.contents.set_item(slot, ItemStack::default());
        }
        for value in entries.iter() {
            let Some(tag) = value.as_compound() else {
                continue;
            };
            let slot = tag.get_byte_or("Slot", 0) as u8;
            if usize::from(slot) < self.contents.size()
                && let Ok(stack) = ItemStack::from_nbt(value)
            {
                self.contents.set_item(usize::from(slot), stack);
            }
        }
    }
    /// Port of PlayerEnderChestContainer#storeAsSlots, in ascending slot order.
    pub(crate) fn store_as_slots(&self) -> ferraloom_protocol::Result<ListTag> {
        let mut entries = Vec::new();
        for (slot, stack) in self.contents.items().iter().enumerate() {
            if !stack.is_empty() {
                let mut tag = stack
                    .to_nbt()?
                    .as_compound()
                    .expect("stack map codec")
                    .clone();
                tag.insert("Slot", Tag::Byte(slot as i8));
                entries.push(Tag::Compound(tag));
            }
        }
        Ok(ListTag(entries))
    }
}

impl ServerPlayer {
    /// Port of Player#getEnderChestInventory; storage is independent of menus.
    pub fn get_ender_chest_inventory(&self) -> &PlayerEnderChestContainer {
        &self.ender_chest_inventory
    }
    pub fn get_ender_chest_inventory_mut(&mut self) -> &mut PlayerEnderChestContainer {
        &mut self.ender_chest_inventory
    }
}

/// Port of PlayerEnderChestContainer#setActiveChest, including the retained
/// generation needed when synchronous callbacks replace or unload the chest.
pub fn set_active_chest(
    player: EntityRef,
    chest: BlockEntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let old = level
        .entity::<ServerPlayer>(player)?
        .ender_chest_inventory
        .active_chest;
    if old == Some(chest) {
        return Ok(());
    }
    level.block_entity::<EnderChestBlockEntity>(chest)?;
    level.retain_block_entity(chest)?;
    level
        .entity_mut::<ServerPlayer>(player)?
        .ender_chest_inventory
        .active_chest = Some(chest);
    if let Some(old) = old {
        level.release_block_entity(old);
    }
    Ok(())
}

/// PlayerEnderChestContainer#stopOpen clears activeChest after the block entity's
/// stopOpen callback; the block/menu owner calls this at that native point.
pub fn clear_active_chest(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    if let Some(chest) = level
        .entity_mut::<ServerPlayer>(player)?
        .ender_chest_inventory
        .active_chest
        .take()
    {
        level.release_block_entity(chest);
    }
    Ok(())
}

/// Port of PlayerEnderChestContainer#stillValid and EnderChestBlockEntity's
/// inherited identity-and-distance check. SimpleContainer itself is always valid.
pub fn still_valid(player: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let owner = level.entity::<ServerPlayer>(player)?;
    match owner.get_ender_chest_inventory().active_chest() {
        None => Ok(true),
        Some(chest) => Ok(level.block_entity_ref(chest.pos)? == Some(chest)
            && owner.is_within_block_interaction_range(chest.pos, 4.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{EntityState, player::GameType},
        inventory::{StackOperation, StackValue},
    };
    use ferraloom_data::{Block, Item};
    use ferraloom_nbt::{CompoundTag, parse_compound};
    use ferraloom_util::{BlockPos, Uuid, Vec3};

    #[test]
    fn native_ender_slots_decode_clamp_save_and_keep_the_active_chest() {
        let mut checked = 0;
        for row in include_str!("../../../tests/java/entity/slots/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("ender-nbt\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            let input = parse_compound(fields[2]).unwrap();
            let mut container = PlayerEnderChestContainer::default();
            container
                .contents
                .set_item(0, ItemStack::new(Item::DIAMOND, 5, Default::default()));
            container
                .contents
                .set_item(26, ItemStack::new(Item::STONE, 3, Default::default()));
            let active = BlockEntityRef {
                pos: BlockPos::ZERO,
                generation: 1,
            };
            container.active_chest = Some(active);
            let before = container.contents.times_changed;
            container.from_slots(input.get_list("EnderItems").unwrap_or(&ListTag::default()));
            let mut output = CompoundTag::new();
            output.insert("EnderItems", Tag::List(container.store_as_slots().unwrap()));
            assert_eq!(output, parse_compound(fields[3]).unwrap(), "{row}");
            assert_eq!(
                (container.contents.times_changed - before).to_string(),
                fields[4],
                "{row}"
            );
            assert_eq!(
                container.is_active_chest(active).to_string(),
                fields[5],
                "{row}"
            );
            checked += 1;
        }
        assert_eq!(checked, 10);
    }

    #[test]
    fn active_chest_survives_held_player_retirement_then_releases_with_the_owner() {
        let mut level = crate::entity::tests::level(false);
        let pos = BlockPos::new(8, 64, 8);
        let state = Block::ENDER_CHEST.default_state();
        level.set_block_state(pos, state).unwrap();
        level
            .set_block_entity(crate::block::entity::create(pos, state).unwrap())
            .unwrap();
        let chest = level.block_entity_ref(pos).unwrap().unwrap();
        let player = level.next_entity_ref();
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "ender_owner".into(),
                    1,
                    Vec3::new(8.5, 64.0, 8.5),
                    GameType::Survival,
                ))),
            )
            .unwrap();
        set_active_chest(player, chest, &mut level).unwrap();
        set_active_chest(player, chest, &mut level).unwrap(); // One cache root.
        let access = level
            .entity_behavior(player)
            .unwrap()
            .get_slot(player, 200, &level)
            .unwrap()
            .unwrap();
        {
            let mut operation = StackOperation::new(&mut level);
            operation
                .pin_existing(access.get(operation.level).unwrap())
                .unwrap();
            operation.level.remove_entity(player).unwrap();
            operation.level.remove_block_entity(pos).unwrap();
            operation.level.finish_entity_management();
            operation.level.collect_detached_block_entities();
            assert!(operation.level.entity_state(player).is_ok());
            assert!(
                operation
                    .level
                    .block_entity::<EnderChestBlockEntity>(chest)
                    .is_ok()
            );
            let accepted = access
                .set(
                    StackValue::New(ItemStack::new(Item::DIAMOND, 7, Default::default())),
                    &mut operation,
                )
                .unwrap();
            let current = operation
                .level
                .stack(access.get(operation.level).unwrap())
                .unwrap();
            let actual = format!(
                "slot-retired-ender\t{}\t{accepted}\t{}:{}\t{}",
                operation
                    .level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .removed,
                current.item().name(),
                crate::entity::player::inventory::count(current),
                operation
                    .level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .get_ender_chest_inventory()
                    .is_active_chest(chest)
            );
            assert_eq!(
                actual,
                include_str!("../../../tests/java/entity/slots/vectors.tsv")
                    .lines()
                    .find(|row| row.starts_with("slot-retired-ender\t"))
                    .unwrap()
            );
            assert!(!still_valid(player, operation.level).unwrap());
        }
        level.finish_entity_management();
        assert!(level.entity_state(player).is_err());
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(chest).is_err());
    }

    #[test]
    fn replacing_and_clearing_active_chest_releases_each_counted_generation() {
        let mut level = crate::entity::tests::level(false);
        let player = level.next_entity_ref();
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "ender_owner".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let mut chests = Vec::new();
        for x in [8, 9] {
            let pos = BlockPos::new(x, 64, 8);
            let state = Block::ENDER_CHEST.default_state();
            level.set_block_state(pos, state).unwrap();
            level
                .set_block_entity(crate::block::entity::create(pos, state).unwrap())
                .unwrap();
            chests.push(level.block_entity_ref(pos).unwrap().unwrap());
        }
        set_active_chest(player, chests[0], &mut level).unwrap();
        set_active_chest(player, chests[1], &mut level).unwrap();
        for chest in &chests {
            level.remove_block_entity(chest.pos).unwrap();
        }
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(chests[0]).is_err());
        assert!(level.block_entity_state(chests[1]).is_ok());
        clear_active_chest(player, &mut level).unwrap();
        clear_active_chest(player, &mut level).unwrap();
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(chests[1]).is_err());
    }
}
