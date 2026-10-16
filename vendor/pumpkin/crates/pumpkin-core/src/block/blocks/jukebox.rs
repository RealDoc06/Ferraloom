use std::sync::Arc;

use crate::block::{
    BlockBehaviour, EmitsRedstonePowerArgs, GetComparatorOutputArgs, GetRedstonePowerArgs,
    NormalUseArgs, PlacedArgs, UseWithItemArgs, entities::jukebox::JukeboxBlockEntity,
    registry::BlockActionResult,
};
use pumpkin_data::{
    block_properties::JukeboxLikeProperties,
    data_component_impl::{EquipmentSlot, JukeboxPlayableImpl},
};
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_macros::pumpkin_block;

#[pumpkin_block("minecraft:jukebox")]
pub struct JukeboxBlock;

impl BlockBehaviour for JukeboxBlock {
    fn placed(&self, args: PlacedArgs<'_>) {
        // The registry creates the BE before calling placed. Replacing that
        // fresh allocation would manufacture a terminal stop on empty placement.
        if args
            .world
            .published_block_entity(args.position)
            .is_some_and(|entity| entity.as_any().is::<JukeboxBlockEntity>())
        {
            return;
        }
        args.world
            .add_block_entity(Arc::new(JukeboxBlockEntity::new(*args.position)));
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if JukeboxLikeProperties::from_state_id(args.world.get_block_state_id(args.position))
            .has_record
            && let Some(entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox) = entity.as_any().downcast_ref::<JukeboxBlockEntity>()
        {
            jukebox.eject_record();
            return BlockActionResult::Success;
        }
        BlockActionResult::Pass
    }

    fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        if JukeboxLikeProperties::from_state_id(args.world.get_block_state_id(args.position))
            .has_record
            || args.item_stack.is_empty()
            || args
                .item_stack
                .get_data_component::<JukeboxPlayableImpl>()
                .is_none()
        {
            return BlockActionResult::PassToDefaultBlockAction;
        }
        // Official consumesAndReturn(1, player) precedes the BE lookup. The
        // missing-BE case consumes and awards PlayRecord but creates no effects.
        let inventory = args.player.inventory();
        let original = args.item_stack.clone();
        let mode = args.player.gamemode.load();
        let (slot, record) = if *args.equipment_slot == EquipmentSlot::MAIN_HAND {
            let slot = inventory.get_selected_slot() as usize;
            let mut items = inventory
                .main_inventory
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !items[slot].are_equal(&original) {
                return BlockActionResult::Success;
            }
            let record = items[slot].split_unless_creative(mode, 1);
            *args.item_stack = items[slot].clone();
            (slot, record)
        } else {
            let mut equipment = inventory
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut actual = equipment.get(&EquipmentSlot::OFF_HAND);
            if !actual.are_equal(&original) {
                return BlockActionResult::Success;
            }
            let record = actual.split_unless_creative(mode, 1);
            equipment.put(&EquipmentSlot::OFF_HAND, actual.clone());
            *args.item_stack = actual;
            (PlayerInventory::OFF_HAND_SLOT, record)
        };
        // Commit and synchronize the original slot before any gameplay callback
        // can change the selected slot or replace the hand's contents.
        if !args.item_stack.are_equal(&original) {
            args.player.sync_hand_slot(slot, args.item_stack.clone());
        }
        if let Some(entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox) = entity.as_any().downcast_ref::<JukeboxBlockEntity>()
        {
            jukebox.set_record(record);
            if args.world.is_current_block_entity(jukebox) {
                // The existing hook carries position only, not vanilla context
                // or sculk dispatch. This is the helper's second notification.
                args.world
                    .emit_game_event("block_change", args.position.to_centered_f64());
            }
        }
        args.player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::PlayRecord as i32,
            1,
        );
        BlockActionResult::Success
    }

    // Physical removal is owned by the BE's on_block_replaced hook, so break,
    // piston/replacement and explicit Inventory removal cannot double-drop.
    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        if args
            .world
            .get_block_entity(args.position)
            .is_some_and(|entity| {
                entity
                    .as_any()
                    .downcast_ref::<JukeboxBlockEntity>()
                    .is_some_and(JukeboxBlockEntity::is_playing)
            })
        {
            15
        } else {
            0
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        Some(
            args.world
                .get_block_entity(args.position)
                .and_then(|entity| {
                    let jukebox = entity.as_any().downcast_ref::<JukeboxBlockEntity>()?;
                    let record = jukebox.get_record();
                    record
                        .get_data_component::<JukeboxPlayableImpl>()
                        .map(|playable| playable.song.comparator_output())
                })
                .unwrap_or(0),
        )
    }
}
