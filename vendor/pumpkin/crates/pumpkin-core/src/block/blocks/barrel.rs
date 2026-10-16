use std::sync::Arc;
use std::sync::Mutex;

use crate::block::{GetComparatorOutputArgs, GetScreenHandlerFactoryArgs, OnPlaceArgs, PlacedArgs};
use crate::block::{
    registry::BlockActionResult,
    {BlockBehaviour, NormalUseArgs},
};

use crate::block::entities::BlockEntity;
use crate::block::entities::barrel::BarrelBlockEntity;
use crate::entity::EntityBase;
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::BarrelLikeProperties;
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::create_generic_9x3;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::text::TextComponent;

struct BarrelScreenFactory {
    inventory: Arc<dyn Inventory>,
    block_entity: Arc<dyn BlockEntity>,
    world: Arc<crate::world::World>,
}

impl ScreenHandlerFactory for BarrelScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let main_hand = player_inventory.held_item();
        if !self
            .block_entity
            .can_open_container(&main_hand, player.is_spectator())
        {
            if !player.is_spectator() {
                super::chests::send_container_locked_notifications(
                    &self.world,
                    self.block_entity.get_position().to_centered_f64(),
                    player,
                    self.get_display_name(),
                );
            }
            return None;
        }
        self.block_entity.unpack_loot_table(&self.world);
        let handler = create_generic_9x3(sync_id, player_inventory, self.inventory.clone(), player);
        let concrete_arc = Arc::new(Mutex::new(handler));

        Some(concrete_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        if let Some(name) = self.block_entity.container_custom_name() {
            return name;
        }
        pumpkin_macros::translate_cross!(
            translation::java::CONTAINER_BARREL,
            translation::bedrock::CONTAINER_BARREL
        )
    }
}

#[pumpkin_block("minecraft:barrel")]
pub struct BarrelBlock;

impl BlockBehaviour for BarrelBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = BarrelLikeProperties::default(args.block);
        props.facing = args.player.get_entity().get_facing().opposite();
        props.to_state_id(args.block)
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::OpenBarrel as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
        }

        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        barrel_screen_handler_factory(args.world, args.position)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let barrel_block_entity = BarrelBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(barrel_block_entity));
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        crate::block::container_comparator_output(&args)
    }
}

/// Retains the actual entity so titles, locks and pending loot reflect live edits.
pub(crate) fn barrel_screen_handler_factory(
    world: &Arc<crate::world::World>,
    position: &pumpkin_util::math::position::BlockPos,
) -> Option<Box<dyn ScreenHandlerFactory>> {
    let block_entity = world.get_block_entity(position)?;
    let inventory = block_entity.clone().get_menu_inventory(world)?;
    Some(Box::new(BarrelScreenFactory {
        inventory,
        block_entity,
        world: world.clone(),
    }))
}
