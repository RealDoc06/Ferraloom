//! Focused Rust dispatcher/persistence coverage, not official paired acceptance.
//! Container locks cover a bounded predicate subset; opaque tags are retained.

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, Weak, mpsc},
    time::Duration,
};

use arc_swap::ArcSwap;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_config::{CommandsConfig, world::LevelConfig};
use pumpkin_data::{
    Block, BlockDirection, BlockId, BlockStateId,
    block_properties::{
        BarrelLikeProperties, ChestLikeProperties, ChestType, ComparatorLikeProperties, Facing,
        FacingHopper, HopperLikeProperties, HorizontalFacing,
    },
    dimension::Dimension,
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_inventory::{
    Inventory, SimpleInventory,
    double::DoubleInventory,
    entity_equipment::EntityEquipment,
    generic_container_screen_handler::{create_generic_9x3, create_generic_9x6},
    player::player_inventory::PlayerInventory,
    screen_handler::{InventoryPlayer, ScreenHandler, ScreenHandlerFactory, SharedScreenHandler},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::{
    GameMode,
    math::{position::BlockPos, vector2::Vector2},
    permission::PermissionManager,
    world_seed::Seed,
};
use pumpkin_world::{
    chunk::{ChunkData, format::anvil::SingleChunkDataSerializer, io::Dirtiable},
    level::Level,
    world_info::LevelData,
};

use super::ChestBlockEntity;
use crate::{
    block::{
        BlockBehaviour, BlockMetadata, GetRedstonePowerArgs, OnNeighborUpdateArgs,
        OnSyncedBlockEventArgs,
        blocks::{
            barrel::{BarrelBlock, barrel_screen_handler_factory},
            chests::{ChestBlock, TrappedChestBlock, chest_screen_handler_factory},
        },
        entities::{
            BlockEntity, barrel::BarrelBlockEntity, comparator::ComparatorBlockEntity,
            hopper::HopperBlockEntity, trapped_chest::TrappedChestBlockEntity,
        },
        registry::default_registry,
    },
    command::{CommandSource, commands::default_dispatcher, node::dispatcher::CommandDispatcher},
    entity::player::Player,
    world::{BlockFlags, World, loot::fill_chest_inventory_handle},
};

const BLOCK_POS: BlockPos = BlockPos::new(0, 64, 0);
const OTHER_POS: BlockPos = BlockPos::new(1, 64, 0);

#[derive(Clone, Copy)]
enum ChestKind {
    Normal,
    Trapped,
    Barrel,
}

const CHESTS: [ChestKind; 2] = [ChestKind::Normal, ChestKind::Trapped];
const CONTAINERS: [ChestKind; 3] = [ChestKind::Normal, ChestKind::Trapped, ChestKind::Barrel];

impl ChestKind {
    fn factory(
        self,
        world: &Arc<World>,
        position: &BlockPos,
    ) -> Box<dyn pumpkin_inventory::screen_handler::ScreenHandlerFactory> {
        match self {
            Self::Normal | Self::Trapped => chest_screen_handler_factory(world, position),
            Self::Barrel => barrel_screen_handler_factory(world, position),
        }
        .unwrap()
    }

    fn block(self) -> &'static Block {
        match self {
            Self::Normal => &Block::CHEST,
            Self::Trapped => &Block::TRAPPED_CHEST,
            Self::Barrel => &Block::BARREL,
        }
    }

    fn create(self, position: BlockPos) -> Arc<dyn BlockEntity> {
        match self {
            Self::Normal => Arc::new(ChestBlockEntity::new(position)),
            Self::Trapped => Arc::new(TrappedChestBlockEntity::new(position)),
            Self::Barrel => Arc::new(BarrelBlockEntity::new(position)),
        }
    }

    fn load(self, nbt: &NbtCompound, position: BlockPos) -> Arc<dyn BlockEntity> {
        match self {
            Self::Normal => Arc::new(ChestBlockEntity::from_nbt(nbt, position)),
            Self::Trapped => Arc::new(TrappedChestBlockEntity::from_nbt(nbt, position)),
            Self::Barrel => Arc::new(BarrelBlockEntity::from_nbt(nbt, position)),
        }
    }

    fn on_generated(self, entity: &dyn BlockEntity, hook: impl FnOnce() + Send + Sync + 'static) {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .set_loot_generated_hook(hook),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .set_loot_generated_hook(hook),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .set_loot_generated_hook(hook),
        }
    }

    fn before_access(self, entity: &dyn BlockEntity, hook: impl FnOnce() + Send + Sync + 'static) {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .set_before_access_hook(hook),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .set_before_access_hook(hook),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .set_before_access_hook(hook),
        }
    }

    fn on_edit(self, entity: &dyn BlockEntity, hook: impl FnOnce() + Send + Sync + 'static) {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .set_edit_hook(hook),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .set_edit_hook(hook),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .set_edit_hook(hook),
        }
    }

    fn on_snapshot(self, entity: &dyn BlockEntity, hook: impl FnOnce() + Send + Sync + 'static) {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .set_snapshot_hook(hook),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .set_snapshot_hook(hook),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .set_snapshot_hook(hook),
        }
    }

    fn state_is_locked(self, entity: &dyn BlockEntity) -> bool {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .state_is_locked(),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .state_is_locked(),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .state_is_locked(),
        }
    }

    fn token(self, entity: &dyn BlockEntity) -> Arc<()> {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .load_token(),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .load_token(),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .load_token(),
        }
    }

    fn viewers(self, entity: &dyn BlockEntity) -> u16 {
        match self {
            Self::Normal => entity
                .as_any()
                .downcast_ref::<ChestBlockEntity>()
                .unwrap()
                .get_viewer_count(),
            Self::Trapped => entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
                .unwrap()
                .get_viewer_count(),
            Self::Barrel => entity
                .as_any()
                .downcast_ref::<BarrelBlockEntity>()
                .unwrap()
                .get_viewer_count(),
        }
    }
}

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(value) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected a compound fixture");
    };
    value
}

fn saved(entity: &dyn BlockEntity) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    entity.write_internal(&mut nbt);
    nbt
}

fn assert_marker(stack: &ItemStack) {
    let mut nbt = NbtCompound::new();
    stack.write_item_stack(&mut nbt);
    assert_eq!(
        nbt.get_compound("components")
            .unwrap()
            .get_compound("minecraft:custom_data"),
        Some(&compound("{marker:17}"))
    );
}

struct Fixture {
    source: CommandSource,
    dispatcher: CommandDispatcher,
    _folder: tempfile::TempDir,
}

impl Fixture {
    /// Same manually loaded Full-chunk pattern as `fill_tests`. No listener,
    /// server tick or authentication is started; stop workers before installing it.
    async fn new() -> Self {
        Self::with_registry(default_registry()).await
    }

    async fn with_registry(registry: Arc<crate::block::registry::BlockRegistry>) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            folder.path().into(),
            0,
            Dimension::OVERWORLD,
        );
        level.shutdown().await.unwrap();
        level
            .loaded_chunks
            .insert(Vector2::new(0, 0), ChunkData::empty_sync(0, 0));
        let world = Arc::new(World::load(
            level,
            Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
            Dimension::OVERWORLD,
            registry,
            Weak::new(),
            Arc::default(),
        ));
        Self {
            source: CommandSource::dummy().with_world(world).with_silent(),
            dispatcher: default_dispatcher(&PermissionManager::new(), &CommandsConfig::default()),
            _folder: folder,
        }
    }

    fn execute(&self, input: &str) -> i32 {
        self.dispatcher
            .execute_input(input, &self.source)
            .unwrap_or_else(|error| panic!("{input}: {error:?}"))
    }

    fn set_state(&self, position: BlockPos, state: BlockStateId) {
        let relative = position.chunk_relative_position();
        self.source
            .world()
            .level
            .read_chunk_sync(&position.chunk_position(), |chunk| {
                chunk.set_block_absolute_y(
                    relative.x as usize,
                    relative.y,
                    relative.z as usize,
                    state,
                );
            })
            .unwrap();
    }

    fn install(&self, block: &Block, entity: Arc<dyn BlockEntity>) {
        self.set_state(entity.get_position(), block.default_state.id);
        self.source.world().add_block_entity(entity);
    }

    fn pending(&self, position: BlockPos) -> NbtCompound {
        self.source
            .world()
            .level
            .read_chunk_sync(&position.chunk_position(), |chunk| {
                chunk.pending_block_entities.lock().unwrap()[&position].clone()
            })
            .unwrap()
    }

    /// Exercise actual terrain serialization and normal lazy entity construction.
    /// Test-only retirement deliberately keeps old menu Arcs alive.
    fn reload_saved_chunk(&self, position: BlockPos) -> Arc<dyn BlockEntity> {
        let world = self.source.world();
        let chunk_pos = position.chunk_position();
        let bytes = world
            .level
            .read_chunk_sync(&chunk_pos, |chunk| chunk.to_bytes().unwrap())
            .unwrap();
        let chunk = ChunkData::from_bytes(&bytes, chunk_pos).unwrap();
        world.block_entities.remove(&chunk_pos);
        world.level.loaded_chunks.insert(chunk_pos, Arc::new(chunk));
        world.get_block_entity(&position).unwrap()
    }
}

// A real menu needs a player inventory, but these tests need no network client.
struct MenuPlayer(Arc<PlayerInventory>, bool);

impl MenuPlayer {
    fn new() -> Self {
        Self(
            Arc::new(PlayerInventory::new(
                Arc::new(Mutex::new(EntityEquipment::new())),
                Arc::new(
                    std::iter::once((
                        PlayerInventory::OFF_HAND_SLOT,
                        pumpkin_data::data_component_impl::EquipmentSlot::OFF_HAND,
                    ))
                    .collect(),
                ),
            )),
            false,
        )
    }
}

impl InventoryPlayer for MenuPlayer {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn drop_item(&self, _item: ItemStack, _retain_ownership: bool) {}
    fn get_inventory(&self) -> Arc<PlayerInventory> {
        self.0.clone()
    }
    fn has_infinite_materials(&self) -> bool {
        false
    }
    fn is_creative(&self) -> bool {
        false
    }
    fn is_spectator(&self) -> bool {
        self.1
    }
    fn experience_level(&self) -> i32 {
        0
    }
    fn add_experience_levels(&self, _levels: i32) {}
    fn enchantment_seed(&self) -> i32 {
        0
    }
    fn set_enchantment_seed(&self, _seed: i32) {}
    fn enqueue_inventory_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetContainerContent,
        _window_type: Option<pumpkin_data::screen::WindowType>,
    ) {
    }
    fn enqueue_slot_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetContainerSlot,
        _window_type: Option<pumpkin_data::screen::WindowType>,
        _total_slots: usize,
    ) {
    }
    fn enqueue_cursor_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetCursorItem,
    ) {
    }
    fn enqueue_property_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetContainerProperty,
    ) {
    }
    fn enqueue_slot_set_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetPlayerInventory,
    ) {
    }
    fn enqueue_set_held_item_packet(
        &self,
        _packet: &pumpkin_protocol::java::client::play::CSetSelectedSlot,
    ) {
    }
    fn enqueue_equipment_change(
        &self,
        _slot: &pumpkin_data::data_component_impl::EquipmentSlot,
        _stack: &ItemStack,
    ) {
    }
    fn award_experience(&self, _amount: i32) {}
    fn increment_stat(
        &self,
        _category: pumpkin_data::statistic::StatisticCategory,
        _stat_id: i32,
        _amount: i32,
    ) {
    }
    fn play_block_sound(&self, _sound: pumpkin_data::sound::Sound, _pitch: f32) {}
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep saved-byte admission, menu, hopper, drop and reload assertions in one fixture."
)]
async fn supported_saved_container_potions_survive_menu_hopper_drop_and_reload() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        // A normal named potion and authored, modeled custom effects. Hidden
        // details and unknown wire payloads are deliberately outside this control.
        let mut nbt = compound(
            "{Items:[\
            {Slot:0b,id:'minecraft:potion',count:1,components:{'minecraft:potion_contents':{potion:'minecraft:swiftness'}}},\
            {Slot:1b,id:'minecraft:tipped_arrow',count:3,components:{'minecraft:potion_contents':{potion:'minecraft:swiftness',custom_color:12345,custom_effects:[{id:'minecraft:speed',amplifier:1b,duration:400,ambient:0b,show_particles:1b,show_icon:1b}]},'minecraft:custom_data':{marker:17}}},\
            {Slot:2b,id:'minecraft:potion',count:1,components:{'minecraft:potion_contents':{custom_effects:[{id:'minecraft:strength',duration:200}]}}}\
        ]}",
        );
        nbt.put_string(
            "id",
            match kind {
                ChestKind::Normal => ChestBlockEntity::ID,
                ChestKind::Trapped => TrappedChestBlockEntity::ID,
                ChestKind::Barrel => BarrelBlockEntity::ID,
            }
            .into(),
        );
        nbt.put_int("x", BLOCK_POS.0.x);
        nbt.put_int("y", BLOCK_POS.0.y);
        nbt.put_int("z", BLOCK_POS.0.z);
        let expected: Vec<_> = nbt
            .get_list("Items")
            .unwrap()
            .iter()
            .map(|tag| ItemStack::read_item_stack(tag.extract_compound().unwrap()).unwrap())
            .collect();
        fixture.set_state(BLOCK_POS, kind.block().default_state.id);
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                chunk
                    .pending_block_entities
                    .lock()
                    .unwrap()
                    .insert(BLOCK_POS, nbt.clone());
            })
            .unwrap();
        // Admit actual saved bytes before the normal lazy constructor and its
        // first published snapshot. No direct from_nbt shortcut for these items.
        let container = fixture.reload_saved_chunk(BLOCK_POS);
        world.update_block_entity(&container);
        let player = MenuPlayer::new();
        let menu = kind
            .factory(world, &BLOCK_POS)
            .create_screen_handler(1, &player.0, &player)
            .unwrap();
        {
            let mut menu = menu.lock().unwrap();
            for (slot, expected) in expected.iter().enumerate() {
                assert!(
                    menu.get_behaviour().slots[slot]
                        .get_stack()
                        .are_equal(expected)
                );
            }
            menu.on_closed(&player);
        };

        let hopper_pos = BLOCK_POS.down();
        let hopper: Arc<dyn BlockEntity> =
            Arc::new(HopperBlockEntity::new(hopper_pos, FacingHopper::Down));
        fixture.install(&Block::HOPPER, hopper.clone());
        let mut properties = HopperLikeProperties::from_state_id(Block::HOPPER.default_state.id);
        properties.enabled = true;
        properties.facing = FacingHopper::Down;
        fixture.set_state(hopper_pos, properties.to_state_id(&Block::HOPPER));
        hopper.tick(world);
        assert!(
            hopper
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .are_equal(&expected[0])
        );
        assert!(
            container
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .is_empty()
        );
        world.update_block_entity(&container);
        world.update_block_entity(&hopper);
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        let inventory = reloaded.get_inventory().unwrap();
        assert!(inventory.get_stack(0).is_empty());
        for (slot, expected) in expected.iter().enumerate().skip(1) {
            assert!(inventory.get_stack(slot).are_equal(expected));
        }
        let reloaded_hopper = world.get_block_entity(&hopper_pos).unwrap();
        assert!(
            reloaded_hopper
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .are_equal(&expected[0])
        );

        // Actual replacement scatters the remaining inventory. Retire spawned
        // entities after collecting stacks so the fixture holds no World cycle.
        world.set_block_state(
            &BLOCK_POS,
            Block::AIR.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        let spawned = world.entities.load_full();
        let dropped: Vec<_> = spawned
            .iter()
            .map(|entity| {
                entity
                    .get_item_entity()
                    .unwrap()
                    .get_item_stack()
                    .lock()
                    .unwrap()
                    .clone()
            })
            .collect();
        for entity in spawned.iter() {
            world.remove_entity(entity.as_ref());
        }
        drop(spawned);
        assert_eq!(dropped.len(), 2);
        for expected in &expected[1..] {
            assert!(dropped.iter().any(|stack| stack.are_equal(expected)));
        }
        world.update_block_entity(&reloaded_hopper);
        let final_hopper = fixture.reload_saved_chunk(hopper_pos);
        assert!(
            final_hopper
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .are_equal(&expected[0])
        );
        assert!(world.get_block_entity(&BLOCK_POS).is_none());
        assert!(
            world
                .level
                .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                    !chunk
                        .pending_block_entities
                        .lock()
                        .unwrap()
                        .contains_key(&BLOCK_POS)
                })
                .unwrap()
        );
    }
}

#[tokio::test]
async fn chest_data_updates_retained_single_menus_and_full_saved_state() {
    for kind in CHESTS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let chest = kind.create(BLOCK_POS);
        let inventory = chest.clone().get_inventory().unwrap();
        inventory.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        inventory.set_stack(26, ItemStack::new(1, &Item::APPLE));
        fixture.install(kind.block(), chest.clone());
        let player = MenuPlayer::new();
        let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
        assert_eq!(handler.get_behaviour().slots[0].get_stack().item_count, 3);
        assert_eq!(kind.viewers(chest.as_ref()), 1);

        // A real neighboring comparator must be scheduled by command completion,
        // after load_nbt has released its inventory and loot locks.
        let comparator_pos = BlockPos::new(1, 64, 0);
        fixture.set_state(comparator_pos.down(), Block::STONE.default_state.id);
        let mut properties =
            ComparatorLikeProperties::from_state_id(Block::COMPARATOR.default_state.id);
        properties.facing = HorizontalFacing::West;
        fixture.install(
            &Block::COMPARATOR,
            Arc::new(ComparatorBlockEntity::new(comparator_pos)),
        );
        fixture.set_state(comparator_pos, properties.to_state_id(&Block::COMPARATOR));
        assert!(!world.is_block_tick_scheduled(&comparator_pos, &Block::COMPARATOR));
        chest.clear_dirty();
        chest.clear_comparator_dirty();
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.mark_dirty(false))
            .unwrap();
        world
            .custom_block_entity_data
            .insert(BLOCK_POS, compound("{retained:9}"));

        assert_eq!(fixture.execute("data modify block 0 64 0 Items set value [{Slot:1b,id:'minecraft:emerald',count:32,components:{'minecraft:custom_data':{marker:17}}}]"), 1);
        assert!(Arc::ptr_eq(
            &chest,
            &world.get_block_entity(&BLOCK_POS).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &inventory,
            &world
                .get_block_entity(&BLOCK_POS)
                .unwrap()
                .get_inventory()
                .unwrap()
        ));
        assert!(Arc::ptr_eq(&inventory, &handler.inventory));
        assert!(handler.get_behaviour().slots[0].get_stack().is_empty());
        assert!(handler.get_behaviour().slots[26].get_stack().is_empty());
        let changed = handler.get_behaviour().slots[1].get_stack();
        assert_eq!(changed.item.id, Item::EMERALD.id);
        assert_eq!(changed.item_count, 32);
        assert_marker(&changed);
        assert_eq!(kind.viewers(chest.as_ref()), 1);
        assert!(chest.is_dirty());
        assert!(!chest.is_comparator_dirty()); // World consumed the notification.
        assert!(world.is_block_tick_scheduled(&comparator_pos, &Block::COMPARATOR));
        assert!(
            world
                .level
                .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.is_dirty())
                .unwrap()
        );
        assert_eq!(
            crate::block::calculate_comparator_output(inventory.as_ref()),
            1
        );
        assert_eq!(fixture.execute("data get block 0 64 0 Items[0].count"), 32);
        let pending = fixture.pending(BLOCK_POS);
        assert_eq!(
            pending.get_list("Items"),
            saved(chest.as_ref()).get_list("Items")
        );
        assert_eq!(
            pending.get_compound("PumpkinCustomData"),
            Some(&compound("{retained:9}"))
        );

        world.flush_block_updates();
        assert!(
            world
                .take_flushed_block_changes()
                .contains(&(BLOCK_POS, kind.block().default_state.id))
        );
        assert_eq!(
            chest.chunk_data_nbt().unwrap().get_list("Items"),
            pending.get_list("Items")
        );

        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert!(!Arc::ptr_eq(&chest, &reloaded));
        let reloaded_inventory = reloaded.get_inventory().unwrap();
        assert!(reloaded_inventory.get_stack(0).is_empty());
        assert!(reloaded_inventory.get_stack(26).is_empty());
        assert!(reloaded_inventory.get_stack(1).are_equal(&changed));
        assert_marker(&reloaded_inventory.get_stack(1));
        assert!(
            handler.get_behaviour().slots[1]
                .get_stack()
                .are_equal(&changed)
        );
    }
}

#[tokio::test]
async fn chest_data_updates_both_retained_double_halves_and_clears_empty_or_removed_items() {
    for kind in CHESTS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let first = kind.create(BLOCK_POS);
        let second = kind.create(OTHER_POS);
        let first_inventory = first.clone().get_inventory().unwrap();
        let second_inventory = second.clone().get_inventory().unwrap();
        first_inventory.set_stack(0, ItemStack::new(2, &Item::DIAMOND));
        first_inventory.set_stack(26, ItemStack::new(1, &Item::APPLE));
        second_inventory.set_stack(0, ItemStack::new(4, &Item::GOLD_INGOT));
        fixture.install(kind.block(), first.clone());
        fixture.install(kind.block(), second.clone());
        // Right facing south connects east to Left, both inside the Full chunk.
        let mut properties = ChestLikeProperties::from_state_id(kind.block().default_state.id);
        properties.facing = HorizontalFacing::South;
        properties.r#type = ChestType::Right;
        let first_state = properties.to_state_id(kind.block());
        fixture.set_state(BLOCK_POS, first_state);
        properties.r#type = ChestType::Left;
        let second_state = properties.to_state_id(kind.block());
        fixture.set_state(OTHER_POS, second_state);
        let inventory: Arc<dyn Inventory> =
            DoubleInventory::new(first_inventory.clone(), second_inventory.clone());
        let player = MenuPlayer::new();
        let handler = create_generic_9x6(1, &player.0, inventory.clone(), &player);
        assert_eq!(handler.get_behaviour().slots[27].get_stack().item_count, 4);

        assert_eq!(fixture.execute("data modify block 0 64 0 Items set value [{Slot:1b,id:'minecraft:emerald',count:8,components:{'minecraft:custom_data':{marker:17}}}]"), 1);
        assert!(handler.get_behaviour().slots[0].get_stack().is_empty());
        assert!(handler.get_behaviour().slots[26].get_stack().is_empty());
        assert_marker(&handler.get_behaviour().slots[1].get_stack());
        assert_eq!(inventory.get_stack(1).item_count, 8);
        assert_eq!(handler.get_behaviour().slots[27].get_stack().item_count, 4);
        assert_eq!(fixture.execute("data modify block 1 64 0 Items set value [{Slot:26b,id:'minecraft:iron_ingot',count:9}]"), 1);
        assert!(handler.get_behaviour().slots[27].get_stack().is_empty());
        assert_eq!(handler.get_behaviour().slots[53].get_stack().item_count, 9);
        assert_eq!(inventory.get_stack(53).item.id, Item::IRON_INGOT.id);
        assert_marker(&handler.get_behaviour().slots[1].get_stack());

        // Input identity cannot change either the live type or the block's half.
        assert_eq!(
            fixture.execute("data merge block 0 64 0 {id:'minecraft:barrel',x:99,y:1,z:-8}"),
            1
        );
        let actual = saved(first.as_ref());
        assert_eq!(actual.get_string("id"), Some(first.resource_location()));
        assert_eq!(actual.get_int("x"), Some(0));
        assert_eq!(actual.get_int("y"), Some(64));
        assert_eq!(actual.get_int("z"), Some(0));
        assert_eq!(world.get_block_state_id(&BLOCK_POS), first_state);
        assert_eq!(world.get_block_state_id(&OTHER_POS), second_state);

        assert_eq!(
            fixture.execute("data modify block 0 64 0 Items set value []"),
            1
        );
        assert!((0..27).all(|slot| handler.get_behaviour().slots[slot].get_stack().is_empty()));
        assert_eq!(handler.get_behaviour().slots[53].get_stack().item_count, 9);
        assert_eq!(fixture.execute("data remove block 1 64 0 Items"), 1);
        assert!(inventory.is_empty());
        assert!((0..54).all(|slot| handler.get_behaviour().slots[slot].get_stack().is_empty()));
        assert_eq!(
            crate::block::calculate_comparator_output(inventory.as_ref()),
            0
        );
        assert!(Arc::ptr_eq(&inventory, &handler.inventory));
        for (position, entity, half) in [
            (BLOCK_POS, &first, &first_inventory),
            (OTHER_POS, &second, &second_inventory),
        ] {
            let published = world.get_block_entity(&position).unwrap();
            assert!(Arc::ptr_eq(entity, &published));
            assert!(Arc::ptr_eq(half, &published.get_inventory().unwrap()));
            assert_eq!(kind.viewers(entity.as_ref()), 1);
            assert!(
                fixture
                    .pending(position)
                    .get_list("Items")
                    .unwrap()
                    .is_empty()
            );
        }
        let first_reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert!(first_reloaded.get_inventory().unwrap().is_empty());
        assert!(
            world
                .get_block_entity(&OTHER_POS)
                .unwrap()
                .get_inventory()
                .unwrap()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn chest_data_reloads_pending_loot_seed_defaults_and_saved_generated_items() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let chest = kind.create(BLOCK_POS);
        let inventory = chest.clone().get_inventory().unwrap();
        inventory.set_stack(0, ItemStack::new(2, &Item::DIAMOND));
        fixture.install(kind.block(), chest.clone());
        assert_eq!(fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L,Items:[{Slot:5b,id:'minecraft:apple',count:4}]}"), 1);
        assert!(chest.has_loot_table());
        assert!(saved(chest.as_ref()).get("Items").is_none());
        assert_eq!(fixture.execute("data modify block 0 64 0 LootTable set value 'minecraft:chests/abandoned_mineshaft'"), 1);
        assert_eq!(
            fixture.execute("data modify block 0 64 0 LootTableSeed set value -44L"),
            1
        );
        let pending = fixture.pending(BLOCK_POS);
        assert_eq!(
            pending.get_string("LootTable"),
            Some("minecraft:chests/abandoned_mineshaft")
        );
        assert_eq!(pending.get_long("LootTableSeed"), Some(-44));
        assert!(pending.get("Items").is_none());
        assert!(chest.chunk_data_nbt().unwrap().get("LootTable").is_none());
        assert!(chest.chunk_data_nbt().unwrap().get("Items").is_none());
        assert!(Arc::ptr_eq(
            &chest,
            &fixture.source.world().get_block_entity(&BLOCK_POS).unwrap()
        ));

        // The terrain save must contain the loot fields absent from the client tag.
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert!(!Arc::ptr_eq(&chest, &reloaded));
        let reloaded_data = saved(reloaded.as_ref());
        assert_eq!(
            reloaded_data.get_string("LootTable"),
            pending.get_string("LootTable")
        );
        assert_eq!(reloaded_data.get_long("LootTableSeed"), Some(-44));
        assert!(reloaded_data.get("Items").is_none());
        assert_eq!(fixture.execute("data remove block 0 64 0 LootTableSeed"), 1);
        assert!(saved(reloaded.as_ref()).get("LootTableSeed").is_none());
        assert!(reloaded.has_loot_table());
        assert_eq!(fixture.execute("data remove block 0 64 0 LootTable"), 1);
        assert!(!reloaded.has_loot_table());
        assert!(
            saved(reloaded.as_ref())
                .get_list("Items")
                .unwrap()
                .is_empty()
        );

        assert_eq!(
            fixture.execute("data merge block 0 64 0 {LootTable:'chests/simple_dungeon'}"),
            1
        );
        assert_eq!(
            saved(reloaded.as_ref()).get_string("LootTable"),
            Some("minecraft:chests/simple_dungeon")
        );
        // Use the production generation/commit path, never manual take/fill.
        let world = fixture.source.world();
        reloaded.unpack_loot_table(world);
        assert!(!reloaded.has_loot_table());
        assert!(!reloaded.clone().get_inventory().unwrap().is_empty());
        world.update_block_entity(&reloaded);
        assert!(fixture.pending(BLOCK_POS).get("LootTable").is_none());
        assert!(fixture.pending(BLOCK_POS).get("LootTableSeed").is_none());
        let generated_reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert!(!generated_reloaded.has_loot_table());
        assert_eq!(saved(generated_reloaded.as_ref()), saved(reloaded.as_ref()));
    }
}

#[test]
fn chest_reload_matches_constructor_defaults_and_preserves_runtime_state() {
    // Pinned 26.3 javap references: ChestBlockEntity.loadAdditional,
    // RandomizableContainer.tryLoadLootTable and TagValueInput.getLongOr.
    for kind in CHESTS {
        let live = kind.create(BLOCK_POS);
        let inventory = live.clone().get_inventory().unwrap();
        inventory.on_open();
        for (snbt, expected_loot) in [
            ("{Items:[{Slot:0b,id:'minecraft:diamond',count:3}]}", None),
            (
                "{LootTable:'chests/simple_dungeon',LootTableSeed:91L}",
                Some(("minecraft:chests/simple_dungeon", 91)),
            ),
            (
                "{LootTable:'minecraft:chests/simple_dungeon'}",
                Some(("minecraft:chests/simple_dungeon", 0)),
            ),
            ("{}", None),
            ("{LootTableSeed:88L}", None),
            (
                "{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:-7}",
                Some(("minecraft:chests/simple_dungeon", -7)),
            ),
            (
                "{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:-1.5f}",
                Some(("minecraft:chests/simple_dungeon", -1)),
            ),
            (
                "{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:-1.5d}",
                Some(("minecraft:chests/simple_dungeon", -2)),
            ),
            (
                "{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:'invalid'}",
                Some(("minecraft:chests/simple_dungeon", 0)),
            ),
            (
                "{LootTable:'INVALID KEY',Items:[{Slot:26b,id:'minecraft:apple',count:2}]}",
                None,
            ),
        ] {
            let mut input = compound(snbt);
            input.put_string("id", "minecraft:barrel".into());
            input.put_int("x", 99);
            input.put_int("y", -1);
            input.put_int("z", 88);
            let constructed = kind.load(&input, BLOCK_POS);
            live.clear_dirty();
            live.clear_comparator_dirty();
            live.load_nbt(&input).unwrap();
            assert_eq!(saved(live.as_ref()), saved(constructed.as_ref()), "{snbt}");
            assert_eq!(live.get_position(), BLOCK_POS);
            assert!(Arc::ptr_eq(
                &inventory,
                &live.clone().get_inventory().unwrap()
            ));
            assert_eq!(kind.viewers(live.as_ref()), 1);
            assert!(live.is_dirty());
            assert!(live.is_comparator_dirty());
            assert!(!constructed.is_dirty());
            assert!(!constructed.is_comparator_dirty());
            if snbt.contains("INVALID KEY") {
                assert_eq!(inventory.get_stack(26).item_count, 2);
            }
            for entity in [&live, &constructed] {
                let nbt = saved(entity.as_ref());
                let loot = nbt
                    .get_string("LootTable")
                    .map(|key| (key.to_string(), nbt.get_long("LootTableSeed").unwrap_or(0)));
                assert_eq!(
                    loot,
                    expected_loot.map(|(key, seed)| (key.to_string(), seed)),
                    "{snbt}"
                );
            }
        }
        inventory.on_close();
        assert_eq!(kind.viewers(live.as_ref()), 0);
    }
}

fn stacks(inventory: &dyn Inventory) -> [ItemStack; 27] {
    std::array::from_fn(|slot| inventory.get_stack(slot))
}

fn generated_stacks(world: &World, key: &str, seed: i64) -> [ItemStack; 27] {
    let detached: Arc<dyn Inventory> = Arc::new(SimpleInventory::new(27));
    fill_chest_inventory_handle(&detached, &world.get_loot_table(key).unwrap(), seed);
    let items = stacks(detached.as_ref());
    assert!(items.iter().any(|stack| !stack.is_empty()));
    items
}

fn assert_stacks(inventory: &dyn Inventory, expected: &[ItemStack; 27]) {
    for (slot, expected) in expected.iter().enumerate() {
        let actual = inventory.get_stack(slot);
        if expected.is_empty() {
            assert!(actual.is_empty(), "slot {slot}");
        } else {
            assert!(actual.are_equal(expected), "slot {slot}");
        }
    }
}

#[tokio::test]
async fn pending_chest_loaded_by_world_unpacks_on_first_inventory_access() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        fixture.set_state(BLOCK_POS, kind.block().default_state.id);
        let pending = kind.load(
            &compound("{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"),
            BLOCK_POS,
        );
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                chunk
                    .pending_block_entities
                    .lock()
                    .unwrap()
                    .insert(BLOCK_POS, saved(pending.as_ref()));
            })
            .unwrap();

        // Normal lazy lookup must bind context before a hopper, comparator or
        // command reads the inventory. No menu or /data call supplies it here.
        let live = world.get_block_entity(&BLOCK_POS).unwrap();
        assert!(live.has_loot_table());
        let expected = generated_stacks(world, "minecraft:chests/simple_dungeon", 73);
        let inventory = live.clone().get_inventory().unwrap();
        assert_stacks(inventory.as_ref(), &expected);
        assert!(!live.has_loot_table());
        world.update_block_entity(&live);
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert_stacks(reloaded.get_inventory().unwrap().as_ref(), &expected);
    }
}

#[tokio::test]
async fn published_pending_chest_unpacks_before_first_inventory_deposit() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let live = kind.load(
            &compound("{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"),
            BLOCK_POS,
        );
        fixture.install(kind.block(), live.clone());
        assert_eq!(
            fixture.pending(BLOCK_POS).get_string("LootTable"),
            Some("minecraft:chests/simple_dungeon")
        );
        assert_eq!(
            fixture.pending(BLOCK_POS).get_long("LootTableSeed"),
            Some(73)
        );
        let inventory = live.clone().get_inventory().unwrap();
        let deposit = ItemStack::new(11, &Item::EMERALD);

        // add_block_entity is the only binding path before this first access.
        inventory.set_stack(26, deposit.clone());
        let mut expected = generated_stacks(world, "minecraft:chests/simple_dungeon", 73);
        expected[26] = deposit;
        assert_stacks(inventory.as_ref(), &expected);
        assert!(!live.has_loot_table());
        world.update_block_entity(&live);
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert_stacks(reloaded.get_inventory().unwrap().as_ref(), &expected);
    }
}

#[tokio::test]
async fn chest_retained_menu_access_unpacks_data_loot_and_persists_deposits() {
    #[derive(Debug)]
    enum Access {
        Get,
        IsEmpty,
        Set,
        Remove,
        RemoveSpecific,
    }
    for kind in CONTAINERS {
        for access in [
            Access::Get,
            Access::IsEmpty,
            Access::Set,
            Access::Remove,
            Access::RemoveSpecific,
        ] {
            let fixture = Fixture::new().await;
            let world = fixture.source.world();
            let chest = kind.create(BLOCK_POS);
            fixture.install(kind.block(), chest.clone());
            let inventory = chest.clone().get_inventory().unwrap();
            let player = MenuPlayer::new();
            let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
            assert_eq!(kind.viewers(chest.as_ref()), 1);
            assert_eq!(fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L,Items:[{Slot:5b,id:'minecraft:apple',count:99}]}"), 1);
            assert!(chest.has_loot_table());
            let pending = fixture.pending(BLOCK_POS);
            assert!(pending.get("Items").is_none());
            assert_eq!(pending.get_long("LootTableSeed"), Some(73));
            let mut expected = generated_stacks(world, "minecraft:chests/simple_dungeon", 73);
            let occupied = expected.iter().position(|stack| !stack.is_empty()).unwrap();
            let deposit = ItemStack::read_item_stack(&compound("{id:'minecraft:emerald',count:11,components:{'minecraft:custom_data':{marker:17}}}")).unwrap();

            // Each operation is the FIRST inventory access after /data. In
            // particular Set must unpack before it accepts the deposited stack.
            match access {
                Access::Get => assert!(
                    handler.get_behaviour().slots[occupied]
                        .get_stack()
                        .are_equal(&expected[occupied])
                ),
                Access::IsEmpty => assert!(!handler.inventory.is_empty()),
                Access::Set => {
                    handler.get_behaviour().slots[26].set_stack(deposit.clone());
                    expected[26] = deposit.clone();
                }
                Access::Remove => {
                    assert!(
                        handler
                            .inventory
                            .remove_stack(occupied)
                            .are_equal(&expected[occupied])
                    );
                    expected[occupied] = ItemStack::EMPTY.clone();
                }
                Access::RemoveSpecific => {
                    let removed = expected[occupied].split(1);
                    assert!(
                        handler.get_behaviour().slots[occupied]
                            .take_stack(1)
                            .are_equal(&removed)
                    );
                }
            }
            assert!(!chest.has_loot_table(), "{access:?}");
            assert_stacks(handler.inventory.as_ref(), &expected);
            handler.get_behaviour().slots[26].set_stack(deposit.clone());
            expected[26] = deposit;
            assert_eq!(kind.viewers(chest.as_ref()), 1);
            assert!(Arc::ptr_eq(&handler.inventory, &inventory));
            assert!(Arc::ptr_eq(
                &chest,
                &world.get_block_entity(&BLOCK_POS).unwrap()
            ));

            // Production save snapshots the full live block entity. Serialize the
            // terrain and reconstruct through normal lazy World lookup as well.
            world.update_block_entity(&chest);
            let persisted = fixture.pending(BLOCK_POS);
            assert!(persisted.get("LootTable").is_none());
            assert!(persisted.get("LootTableSeed").is_none());
            let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
            assert!(!reloaded.has_loot_table());
            let reloaded_inventory = reloaded.get_inventory().unwrap();
            assert_stacks(reloaded_inventory.as_ref(), &expected);
            assert_marker(&reloaded_inventory.get_stack(26));
            assert_stacks(handler.inventory.as_ref(), &expected);
        }
    }
}

#[tokio::test]
async fn chest_stale_real_generation_cannot_fill_a_data_replacement() {
    for kind in CONTAINERS {
        // Cover explicit Items, a different pending key, and an identical
        // key/seed reload. The last case requires identity rather than equality.
        for replacement in [
            "data merge block 0 64 0 {LootTable:'INVALID KEY',Items:[{Slot:7b,id:'minecraft:emerald',count:11,components:{'minecraft:custom_data':{marker:17}}}]}",
            "data modify block 0 64 0 LootTable set value 'minecraft:chests/abandoned_mineshaft'",
            "data modify block 0 64 0 Items set value [{Slot:7b,id:'minecraft:emerald',count:11}]",
        ] {
            let fixture = Fixture::new().await;
            let world = fixture.source.world();
            let chest = kind.create(BLOCK_POS);
            fixture.install(kind.block(), chest.clone());
            let inventory = chest.clone().get_inventory().unwrap();
            let player = MenuPlayer::new();
            let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
            assert_eq!(fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"), 1);
            let (generated_tx, generated_rx) = mpsc::channel();
            let (resume_tx, resume_rx) = mpsc::channel();
            let resume_rx = Mutex::new(resume_rx);
            kind.on_generated(chest.as_ref(), move || {
                generated_tx.send(()).unwrap();
                // Timeout is only a deadlock guard. The channels enforce the
                // interleaving after actual generation, with no sleeps/retries.
                resume_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            });
            let regenerated = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| handler.get_behaviour().slots[7].get_stack());
                generated_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                // The old result exists, but has not been committed or consumed.
                assert!(chest.has_loot_table());
                assert_eq!(fixture.execute(replacement), 1);
                let pending = chest.has_loot_table();
                let replacement_data = fixture.pending(BLOCK_POS);
                let expected = if pending {
                    generated_stacks(world, replacement_data.get_string("LootTable").unwrap(), 73)
                } else {
                    let mut items = std::array::from_fn(|_| ItemStack::EMPTY.clone());
                    pumpkin_inventory::sync_read_items_from_nbt(&replacement_data, &mut items);
                    assert_eq!(items[7].item.id, Item::EMERALD.id);
                    assert_eq!(items[7].item_count, 11);
                    assert_marker(&items[7]);
                    items
                };
                let flag = regenerated.clone();
                kind.on_generated(chest.as_ref(), move || {
                    flag.store(true, std::sync::atomic::Ordering::Relaxed);
                });
                resume_tx.send(()).unwrap();
                let accessed = worker.join().unwrap();
                assert_eq!(
                    regenerated.load(std::sync::atomic::Ordering::Relaxed),
                    pending
                );
                assert!(accessed.are_equal(&expected[7]));
                assert_stacks(inventory.as_ref(), &expected);
                assert!(!chest.has_loot_table());
                assert_eq!(kind.viewers(chest.as_ref()), 1);
                assert!(Arc::ptr_eq(&handler.inventory, &inventory));
                world.update_block_entity(&chest);
                let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
                assert_stacks(reloaded.get_inventory().unwrap().as_ref(), &expected);
            });
        }
    }
}

#[tokio::test]
async fn chest_menu_insertion_rechecks_loot_loaded_after_ensure_unpacked() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let chest = kind.create(BLOCK_POS);
        fixture.install(kind.block(), chest.clone());
        let inventory = chest.clone().get_inventory().unwrap();
        let player = MenuPlayer::new();
        let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let resume_rx = Mutex::new(resume_rx);
        kind.before_access(chest.as_ref(), move || {
            ready_tx.send(()).unwrap();
            resume_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        });
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                handler.get_behaviour().slots[26].set_stack(ItemStack::new(11, &Item::EMERALD));
            });
            ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            // ensure_unpacked saw no loot. Reload exactly before slot access.
            assert_eq!(fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"), 1);
            assert!(chest.has_loot_table());
            resume_tx.send(()).unwrap();
            worker.join().unwrap();
        });
        let mut expected = generated_stacks(world, "minecraft:chests/simple_dungeon", 73);
        expected[26] = ItemStack::new(11, &Item::EMERALD);
        assert!(!chest.has_loot_table());
        assert_stacks(inventory.as_ref(), &expected);
        assert_eq!(kind.viewers(chest.as_ref()), 1);
        world.update_block_entity(&chest);
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert_stacks(reloaded.get_inventory().unwrap().as_ref(), &expected);
    }
}

#[tokio::test]
async fn chest_pending_loot_refuses_spectator_open_for_single_and_either_double_half() {
    for kind in CHESTS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let first = kind.create(BLOCK_POS);
        let second = kind.create(OTHER_POS);
        fixture.install(kind.block(), first.clone());
        fixture.install(kind.block(), second.clone());
        let mut player = MenuPlayer::new();
        player.1 = true;
        assert_eq!(fixture.execute("data merge block 0 64 0 {LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"), 1);
        let pending = saved(first.as_ref());
        let single = chest_screen_handler_factory(world, &BLOCK_POS).unwrap();
        assert!(
            single
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        assert_eq!(saved(first.as_ref()), pending);
        assert_eq!(kind.viewers(first.as_ref()), 0);
        assert_eq!(fixture.execute("data remove block 0 64 0 LootTable"), 1);
        let mut properties = ChestLikeProperties::from_state_id(kind.block().default_state.id);
        properties.facing = HorizontalFacing::South;
        properties.r#type = ChestType::Right;
        fixture.set_state(BLOCK_POS, properties.to_state_id(kind.block()));
        properties.r#type = ChestType::Left;
        fixture.set_state(OTHER_POS, properties.to_state_id(kind.block()));
        for (position, entity) in [(BLOCK_POS, &first), (OTHER_POS, &second)] {
            assert_eq!(fixture.execute(&format!("data merge block {} 64 0 {{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}}", position.0.x)), 1);
            let pending = saved(entity.as_ref());
            for clicked in [BLOCK_POS, OTHER_POS] {
                let factory = chest_screen_handler_factory(world, &clicked).unwrap();
                assert!(
                    factory
                        .create_screen_handler(1, &player.0, &player)
                        .is_none()
                );
                assert_eq!(saved(entity.as_ref()), pending);
                assert_eq!(kind.viewers(first.as_ref()), 0);
                assert_eq!(kind.viewers(second.as_ref()), 0);
            }
            // Opening through production logic unpacks the pending half and
            // retains both Inventory Arcs in the actual double container menu.
            player.1 = false;
            let factory = chest_screen_handler_factory(world, &BLOCK_POS).unwrap();
            let menu = factory
                .create_screen_handler(1, &player.0, &player)
                .unwrap();
            assert!(!entity.has_loot_table());
            assert!(
                !menu
                    .lock()
                    .unwrap()
                    .get_behaviour()
                    .slots
                    .iter()
                    .take(54)
                    .all(|slot| slot.get_stack().is_empty())
            );
            menu.lock().unwrap().on_closed(&player);
            assert_eq!(kind.viewers(first.as_ref()), 0);
            assert_eq!(kind.viewers(second.as_ref()), 0);
            player.1 = true;
            let spectator = chest_screen_handler_factory(world, &OTHER_POS).unwrap();
            assert!(
                spectator
                    .create_screen_handler(2, &player.0, &player)
                    .is_some()
            );
        }
    }
}

#[test]
fn chest_unbound_pending_inventory_refuses_deposits_without_consuming_loot() {
    for kind in CONTAINERS {
        let chest = kind.load(
            &compound("{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"),
            BLOCK_POS,
        );
        let pending = saved(chest.as_ref());
        let inventory = chest.clone().get_inventory().unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                inventory.set_stack(0, ItemStack::new(11, &Item::EMERALD));
            }))
            .is_err()
        );
        assert_eq!(saved(chest.as_ref()), pending);
        assert!(chest.has_loot_table());
    }
}

const DEADLOCK_GUARD: Duration = Duration::from_secs(10);

/// Spawn unscoped workers so a failed timeout cannot itself block on scope exit.
/// Channels determine the order; timeouts only bound deadlock failures.
fn command_worker(
    fixture: &Fixture,
    command: &str,
) -> (
    std::thread::JoinHandle<()>,
    mpsc::Receiver<Result<i32, String>>,
) {
    let source = fixture.source.clone();
    let command = command.to_owned();
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let dispatcher = default_dispatcher(&PermissionManager::new(), &CommandsConfig::default());
        let result = dispatcher
            .execute_input(&command, &source)
            .map_err(|error| format!("{error:?}"));
        tx.send(result).unwrap();
    });
    (worker, rx)
}

fn pause_hook() -> (
    impl FnOnce() + Send + Sync,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (paused_tx, paused_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let resume_rx = Mutex::new(resume_rx);
    (
        move || {
            paused_tx.send(()).unwrap();
            resume_rx
                .lock()
                .unwrap()
                .recv_timeout(DEADLOCK_GUARD)
                .unwrap();
        },
        paused_rx,
        resume_tx,
    )
}

fn marker_stack() -> ItemStack {
    ItemStack::read_item_stack(&compound(
        "{id:'minecraft:emerald',count:11,components:{'minecraft:custom_data':{marker:17}}}",
    ))
    .unwrap()
}

#[tokio::test]
async fn live_container_command_holds_state_through_edit_and_retained_menu_deposit() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let inventory = live.clone().get_inventory().unwrap();
        inventory.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        let player = MenuPlayer::new();
        let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
        let (hook, paused, resume) = pause_hook();
        kind.on_edit(live.as_ref(), hook);
        let (command, result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set value 5",
        );
        paused.recv_timeout(DEADLOCK_GUARD).unwrap();
        let (enter_tx, enter_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let retained = live.clone();
        let deposit = std::thread::spawn(move || {
            // The real state lock used by this retained menu is contended while
            // the dispatcher is paused after its current-state snapshot.
            enter_tx
                .send(kind.state_is_locked(retained.as_ref()))
                .unwrap();
            handler.get_behaviour().slots[7].set_stack(marker_stack());
            assert!(done_tx.send(handler).is_ok());
        });
        assert!(enter_rx.recv_timeout(DEADLOCK_GUARD).unwrap());
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        resume.send(()).unwrap();
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        let handler = done_rx.recv_timeout(DEADLOCK_GUARD).unwrap();
        command.join().unwrap();
        deposit.join().unwrap();
        assert_eq!(handler.get_behaviour().slots[0].get_stack().item_count, 5);
        assert_marker(&handler.get_behaviour().slots[7].get_stack());
        assert!(Arc::ptr_eq(&handler.inventory, &inventory));
        assert!(Arc::ptr_eq(
            &fixture.source.world().get_block_entity(&BLOCK_POS).unwrap(),
            &live
        ));
        fixture.source.world().update_block_entity(&live);
        let reloaded = fixture
            .reload_saved_chunk(BLOCK_POS)
            .get_inventory()
            .unwrap();
        assert_eq!(reloaded.get_stack(0).item_count, 5);
        assert_marker(&reloaded.get_stack(7));
    }
}

#[tokio::test]
async fn live_container_source_is_resolved_before_target_snapshot_and_independent_edits_progress() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let target = kind.create(BLOCK_POS);
        let source = kind.create(OTHER_POS);
        fixture.install(kind.block(), target.clone());
        fixture.install(kind.block(), source.clone());
        target
            .clone()
            .get_inventory()
            .unwrap()
            .set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        source
            .clone()
            .get_inventory()
            .unwrap()
            .set_stack(0, ItemStack::new(5, &Item::APPLE));
        let (hook, paused, resume) = pause_hook();
        kind.on_snapshot(source.as_ref(), hook);
        let (command, result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set from block 1 64 0 Items[0].count",
        );
        paused.recv_timeout(DEADLOCK_GUARD).unwrap();
        assert!(!kind.state_is_locked(target.as_ref()));
        let player = MenuPlayer::new();
        let inventory = target.clone().get_inventory().unwrap();
        let handler = create_generic_9x3(1, &player.0, inventory, &player);
        handler.get_behaviour().slots[7].set_stack(marker_stack());
        resume.send(()).unwrap();
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        command.join().unwrap();
        assert_eq!(handler.get_behaviour().slots[0].get_stack().item_count, 5);
        assert_marker(&handler.get_behaviour().slots[7].get_stack());
        fixture.source.world().update_block_entity(&target);
        let reloaded = fixture
            .reload_saved_chunk(BLOCK_POS)
            .get_inventory()
            .unwrap();
        assert_eq!(reloaded.get_stack(0).item_count, 5);
        assert_marker(&reloaded.get_stack(7));
    }
}

#[tokio::test]
async fn live_container_older_autosave_snapshot_cannot_publish_after_newer_command() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        live.clone()
            .get_inventory()
            .unwrap()
            .set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        let other = kind.create(OTHER_POS);
        fixture.install(kind.block(), other.clone());
        other
            .get_inventory()
            .unwrap()
            .set_stack(0, ItemStack::new(2, &Item::APPLE));
        let (hook, paused, resume) = pause_hook();
        kind.on_snapshot(live.as_ref(), hook);
        let world = fixture.source.world().clone();
        let (save_tx, save_rx) = mpsc::channel();
        let save = std::thread::spawn(move || {
            world.save_block_entities(BLOCK_POS.chunk_position());
            save_tx.send(()).unwrap();
        });
        paused.recv_timeout(DEADLOCK_GUARD).unwrap();
        assert!(kind.state_is_locked(live.as_ref()));
        // A second entity progresses while A's snapshot holds its own lock.
        let (independent, independent_result) = command_worker(
            &fixture,
            "data modify block 1 64 0 Items[0].count set value 9",
        );
        assert_eq!(
            independent_result
                .recv_timeout(DEADLOCK_GUARD)
                .unwrap()
                .unwrap(),
            1
        );
        independent.join().unwrap();
        let (command, result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set value 5",
        );
        resume.send(()).unwrap();
        save_rx.recv_timeout(DEADLOCK_GUARD).unwrap();
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        save.join().unwrap();
        command.join().unwrap();
        assert_eq!(fixture.pending(BLOCK_POS), saved(live.as_ref()));
        let reloaded = fixture
            .reload_saved_chunk(BLOCK_POS)
            .get_inventory()
            .unwrap();
        assert_eq!(reloaded.get_stack(0).item_count, 5);
    }
}

#[tokio::test]
async fn live_container_failed_and_unchanged_edits_preserve_state_token_pending_and_dirty_flags() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let live = kind.load(
            &compound("{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"),
            BLOCK_POS,
        );
        fixture.install(kind.block(), live.clone());
        let token = kind.token(live.as_ref());
        let before = saved(live.as_ref());
        let pending = fixture.pending(BLOCK_POS);
        live.clear_dirty();
        fixture
            .source
            .world()
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.mark_dirty(false))
            .unwrap();
        for command in [
            "data merge block 0 64 0 {LootTableSeed:73L}",
            "data modify block 0 64 0 created.child merge value 1",
            "data modify block 0 64 0 created.nested merge value {}",
            "data modify block 0 64 0 created.list[0].nested merge value {a:1}",
            "data remove block 0 64 0 absent",
        ] {
            assert!(
                fixture
                    .dispatcher
                    .execute_input(command, &fixture.source)
                    .is_err()
            );
            assert_eq!(saved(live.as_ref()), before);
            assert_eq!(fixture.pending(BLOCK_POS), pending);
            assert!(Arc::ptr_eq(&token, &kind.token(live.as_ref())));
            assert!(!live.is_dirty());
            assert!(
                !fixture
                    .source
                    .world()
                    .level
                    .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.is_dirty())
                    .unwrap()
            );
        }
    }
}

#[tokio::test]
async fn live_container_self_and_opposing_source_commands_complete_without_nested_state_locks() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        for position in [BLOCK_POS, OTHER_POS] {
            let live = kind.create(position);
            fixture.install(kind.block(), live.clone());
            let inventory = live.get_inventory().unwrap();
            inventory.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
            inventory.set_stack(1, ItemStack::new(5, &Item::APPLE));
        }
        let (worker, result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set from block 0 64 0 Items[1].count",
        );
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        worker.join().unwrap();
        let first = fixture.source.world().get_block_entity(&BLOCK_POS).unwrap();
        let second = fixture.source.world().get_block_entity(&OTHER_POS).unwrap();
        let (first_hook, first_paused, first_resume) = pause_hook();
        let (second_hook, second_paused, second_resume) = pause_hook();
        kind.on_snapshot(first.as_ref(), first_hook);
        kind.on_snapshot(second.as_ref(), second_hook);
        let (left, left_result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set from block 1 64 0 Items[0].count",
        );
        let (right, right_result) = command_worker(
            &fixture,
            "data modify block 1 64 0 Items[1].count set from block 0 64 0 Items[1].count",
        );
        // Both real source snapshots exist simultaneously, before either target
        // transaction. No command may hold a target lock while reading its source.
        first_paused.recv_timeout(DEADLOCK_GUARD).unwrap();
        second_paused.recv_timeout(DEADLOCK_GUARD).unwrap();
        first_resume.send(()).unwrap();
        second_resume.send(()).unwrap();
        assert_eq!(
            left_result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(),
            1
        );
        // An unchanged result is allowed for the independent opposite command.
        right_result
            .recv_timeout(DEADLOCK_GUARD)
            .unwrap()
            .unwrap_err();
        left.join().unwrap();
        right.join().unwrap();
    }
}

#[tokio::test]
async fn live_container_comparator_callback_can_reenter_inventory_publication_and_save() {
    use crate::block::{BlockBehaviour, BlockMetadata, OnNeighborUpdateArgs};
    use pumpkin_data::BlockId;

    struct ReentrantComparator(mpsc::Sender<u8>);
    impl BlockMetadata for ReentrantComparator {
        fn ids() -> Box<[BlockId]> {
            [BlockId::COMPARATOR].into()
        }
    }
    impl BlockBehaviour for ReentrantComparator {
        fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
            // Exercise map mutation access too: a forgotten publication guard
            // deadlocks here before the state-lock check below could complete.
            let live = args
                .world
                .block_entities
                .get_mut(&BLOCK_POS.chunk_position())
                .unwrap()
                .get(&BLOCK_POS)
                .unwrap()
                .clone();
            let count = live
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .item_count;
            args.world.update_block_entity(&live);
            self.0.send(count).unwrap();
        }
    }

    for kind in CONTAINERS {
        let (callback_tx, callback_rx) = mpsc::channel();
        let mut registry = default_registry();
        Arc::get_mut(&mut registry)
            .unwrap()
            .register(ReentrantComparator(callback_tx));
        let fixture = Fixture::with_registry(registry).await;
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        live.clone()
            .get_inventory()
            .unwrap()
            .set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        fixture.set_state(OTHER_POS, Block::COMPARATOR.default_state.id);
        let (worker, result) = command_worker(
            &fixture,
            "data modify block 0 64 0 Items[0].count set value 5",
        );
        assert_eq!(callback_rx.recv_timeout(DEADLOCK_GUARD).unwrap(), 5);
        assert_eq!(result.recv_timeout(DEADLOCK_GUARD).unwrap().unwrap(), 1);
        worker.join().unwrap();
        assert_eq!(fixture.pending(BLOCK_POS), saved(live.as_ref()));
        fixture.source.world().flush_block_updates();
        assert!(
            fixture
                .source
                .world()
                .take_flushed_block_changes()
                .contains(&(BLOCK_POS, kind.block().default_state.id))
        );
    }
}

#[tokio::test]
async fn container_metadata_survives_recursive_commands_retained_menu_writes_and_disk_reload() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let initial = compound(
            "{CustomName:{text:'Vault',color:'gold',bold:1b,extra:[{text:'!'}]},lock:{components:{'minecraft:custom_name':'key'}},components:{'minecraft:custom_data':{keep:7,nested:{left:1}}}}",
        );
        let live = kind.load(&initial, BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let inventory = live.clone().get_inventory().unwrap();
        let player = MenuPlayer::new();
        let mut key = ItemStack::new(1, &Item::STICK);
        key.set_custom_name("key".into());
        player.0.set_held_item(key);
        let factory = kind.factory(world, &BLOCK_POS);
        let menu = factory
            .create_screen_handler(1, &player.0, &player)
            .unwrap();
        assert_eq!(factory.get_display_name().get_text(), "Vault!");
        assert_eq!(factory.get_display_name().0.style.bold, Some(true));

        assert_eq!(fixture.execute("data merge block 0 64 0 {CustomName:{text:'Updated'},components:{'minecraft:custom_data':{nested:{right:2}}}}"), 1);
        assert_eq!(factory.get_display_name().get_text(), "Updated!");
        assert_eq!(factory.get_display_name().0.style.bold, Some(true));
        assert_eq!(
            factory.get_display_name().0.style.color,
            Some(pumpkin_util::text::color::Color::Named(
                pumpkin_util::text::color::NamedColor::Gold
            ))
        );
        assert!(Arc::ptr_eq(
            &live,
            &world.get_block_entity(&BLOCK_POS).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &inventory,
            &world
                .get_block_entity(&BLOCK_POS)
                .unwrap()
                .get_inventory()
                .unwrap()
        ));
        assert_eq!(fixture.pending(BLOCK_POS), saved(live.as_ref()));

        // A retained real menu and direct inventory edits share this same entity.
        menu.lock().unwrap().get_behaviour().slots[0].set_stack(ItemStack::new(4, &Item::DIAMOND));
        inventory.set_stack(1, ItemStack::new(3, &Item::APPLE));
        world.update_block_entity(&live);
        let persisted = fixture.pending(BLOCK_POS);
        assert_eq!(persisted.get("lock"), initial.get("lock"));
        assert_eq!(
            persisted.get_compound("components"),
            Some(&compound(
                "{'minecraft:custom_data':{keep:7,nested:{left:1,right:2}}}"
            ))
        );
        menu.lock().unwrap().on_closed(&player);
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert_eq!(saved(reloaded.as_ref()), persisted);
        assert_eq!(
            kind.factory(world, &BLOCK_POS)
                .get_display_name()
                .get_text(),
            "Updated!"
        );
        assert_eq!(
            reloaded
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(0)
                .item_count,
            4
        );
        assert_eq!(
            reloaded
                .clone()
                .get_inventory()
                .unwrap()
                .get_stack(1)
                .item_count,
            3
        );
        assert_eq!(fixture.execute("data remove block 0 64 0 CustomName"), 1);
        assert!(reloaded.container_custom_name().is_none());
        assert!(saved(reloaded.as_ref()).get("CustomName").is_none());
        assert_ne!(
            kind.factory(world, &BLOCK_POS)
                .get_display_name()
                .get_text(),
            "Updated!"
        );
    }
}

#[tokio::test]
async fn container_lock_checks_actual_main_hand_and_spectator_at_menu_creation() {
    // 26.3 LockCode.canUnlock: spectator || predicate(main hand).
    // DataComponentExactPredicate.test compares full component equality, not
    // display text, item_name, or either hand. Creative has no special bypass.
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let factory = kind.factory(world, &BLOCK_POS);
        let mut player = MenuPlayer::new();
        assert_eq!(
            fixture.execute(
                "data merge block 0 64 0 {lock:{components:{'minecraft:custom_name':'key'}}}"
            ),
            1
        );
        assert!(
            factory
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        let mut key = ItemStack::new(1, &Item::STICK);
        key.set_custom_name("key".into());
        player
            .0
            .set_stack_in_hand(pumpkin_util::Hand::Left, key.clone());
        assert_eq!(
            player.0.off_hand_item().get_custom_name(),
            key.get_custom_name()
        );
        assert!(
            factory
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        player.0.set_held_item(
            ItemStack::read_item_stack(&compound(
                "{id:'minecraft:stick',count:1,components:{'minecraft:item_name':'key'}}",
            ))
            .unwrap(),
        );
        assert!(
            factory
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        let mut styled_key = key.clone();
        styled_key.patch = vec![(
            pumpkin_data::data_component::DataComponent::CustomName,
            Some(Box::new(
                pumpkin_data::data_component_impl::CustomNameImpl {
                    name: pumpkin_util::text::TextComponent::text("key").bold(),
                },
            )),
        )];
        player.0.set_held_item(styled_key);
        assert!(
            factory
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        player.0.set_held_item(key);
        let menu = factory
            .create_screen_handler(1, &player.0, &player)
            .unwrap();
        menu.lock().unwrap().on_closed(&player);
        player.0.set_held_item(ItemStack::EMPTY.clone());
        player.1 = true;
        let spectator = factory
            .create_screen_handler(2, &player.0, &player)
            .unwrap();
        spectator.lock().unwrap().on_closed(&player);
        player.1 = false;
        assert_eq!(
            fixture.execute("data modify block 0 64 0 lock set value {}"),
            1
        );
        let unlocked = factory
            .create_screen_handler(3, &player.0, &player)
            .unwrap();
        unlocked.lock().unwrap().on_closed(&player);
        assert_eq!(kind.viewers(live.as_ref()), 0);
    }
}

#[tokio::test]
async fn container_denied_open_never_consumes_loot_or_increments_viewers() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let live = kind.load(&compound("{lock:{components:{'minecraft:custom_name':'key'}},LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}"), BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let before = saved(live.as_ref());
        let token = kind.token(live.as_ref());
        let factory = kind.factory(world, &BLOCK_POS);
        let mut player = MenuPlayer::new();
        assert!(
            factory
                .create_screen_handler(1, &player.0, &player)
                .is_none()
        );
        player.1 = true;
        assert!(
            factory
                .create_screen_handler(2, &player.0, &player)
                .is_none()
        );
        assert_eq!(saved(live.as_ref()), before);
        assert!(Arc::ptr_eq(&token, &kind.token(live.as_ref())));
        assert_eq!(kind.viewers(live.as_ref()), 0);
    }
}

#[tokio::test]
async fn double_container_title_precedence_and_both_locks_follow_live_entities() {
    for kind in CHESTS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let first = kind.load(
            &compound("{CustomName:'Right',lock:{components:{'minecraft:custom_name':'key'}}}"),
            BLOCK_POS,
        );
        let second = kind.load(
            &compound("{CustomName:'Left',lock:{components:{'minecraft:custom_name':'other'}}}"),
            OTHER_POS,
        );
        fixture.install(kind.block(), first.clone());
        fixture.install(kind.block(), second.clone());
        let mut properties = ChestLikeProperties::from_state_id(kind.block().default_state.id);
        properties.facing = HorizontalFacing::South;
        properties.r#type = ChestType::Right;
        fixture.set_state(BLOCK_POS, properties.to_state_id(kind.block()));
        properties.r#type = ChestType::Left;
        fixture.set_state(OTHER_POS, properties.to_state_id(kind.block()));
        let factories = [
            kind.factory(world, &BLOCK_POS),
            kind.factory(world, &OTHER_POS),
        ];
        let player = MenuPlayer::new();
        let mut key = ItemStack::new(1, &Item::STICK);
        key.set_custom_name("key".into());
        player.0.set_held_item(key);
        for factory in &factories {
            assert_eq!(factory.get_display_name().get_text(), "Right");
            assert!(
                factory
                    .create_screen_handler(1, &player.0, &player)
                    .is_none()
            );
        }
        assert_eq!(fixture.execute("data remove block 0 64 0 CustomName"), 1);
        assert_eq!(
            fixture.execute(
                "data modify block 1 64 0 lock.components.'minecraft:custom_name' set value 'key'"
            ),
            1
        );
        for factory in &factories {
            assert_eq!(factory.get_display_name().get_text(), "Left");
            let menu = factory
                .create_screen_handler(2, &player.0, &player)
                .unwrap();
            assert_eq!(menu.lock().unwrap().get_behaviour().slots.len(), 90);
            menu.lock().unwrap().on_closed(&player);
        }
        assert_eq!(kind.viewers(first.as_ref()), 0);
        assert_eq!(kind.viewers(second.as_ref()), 0);
    }
}

#[tokio::test]
async fn opaque_container_metadata_survives_unrelated_edits_and_restart() {
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let world = fixture.source.world();
        let input = compound(
            "{CustomName:{translate:'container.chest',fallback:'Storage'},lock:{items:'#minecraft:planks',count:{min:2},predicates:{'minecraft:custom_data':{key:1}}},components:{'minecraft:custom_data':{unknown:[I;1,2,3]}}}",
        );
        let live = kind.load(&input, BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        assert_eq!(fixture.execute("data modify block 0 64 0 Items set value [{Slot:0b,id:'minecraft:diamond',count:2}]"), 1);
        live.clone()
            .get_inventory()
            .unwrap()
            .set_stack(1, ItemStack::new(3, &Item::APPLE));
        world.update_block_entity(&live);
        for key in ["CustomName", "lock", "components"] {
            assert_eq!(saved(live.as_ref()).get(key), input.get(key));
        }
        assert!(!live.can_open_container(&ItemStack::new(64, &Item::OAK_PLANKS), false));
        assert!(live.can_open_container(ItemStack::EMPTY, true));
        let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
        assert_eq!(saved(reloaded.as_ref()), saved(live.as_ref()));
        assert_eq!(fixture.execute("data remove block 0 64 0 lock"), 1);
        assert!(reloaded.can_open_container(ItemStack::EMPTY, false));
    }
}

#[tokio::test]
async fn noncompound_and_unknown_lock_fields_keep_java_access_and_raw_persistence() {
    // Exact 26.3 codec probe: non-map tags have no partial; unknown record keys
    // are ignored, including alongside a valid custom_name predicate.
    for kind in CONTAINERS {
        for lock in [
            "'legacy string'",
            "1b",
            "[]",
            "{foo:1b}",
            "{foo:1b,components:{'minecraft:custom_name':'key'}}",
        ] {
            let fixture = Fixture::new().await;
            let input = compound(&format!("{{lock:{lock}}}"));
            let live = kind.load(&input, BLOCK_POS);
            fixture.install(kind.block(), live.clone());
            let inventory = live.clone().get_inventory().unwrap();
            let keyed = lock.contains("custom_name");
            let mut key = ItemStack::new(1, &Item::STICK);
            key.set_custom_name("key".into());
            assert_eq!(live.can_open_container(ItemStack::EMPTY, false), !keyed);
            assert!(live.can_open_container(&key, false));
            assert_eq!(
                fixture.execute("data merge block 0 64 0 {CustomName:'Keep'}"),
                1
            );
            inventory.set_stack(0, ItemStack::new(2, &Item::APPLE));
            fixture.source.world().update_block_entity(&live);
            assert!(Arc::ptr_eq(
                &inventory,
                &live.clone().get_inventory().unwrap()
            ));
            assert_eq!(saved(live.as_ref()).get("lock"), input.get("lock"));
            let reloaded = fixture.reload_saved_chunk(BLOCK_POS);
            assert_eq!(saved(reloaded.as_ref()).get("lock"), input.get("lock"));
            assert_eq!(reloaded.can_open_container(ItemStack::EMPTY, false), !keyed);
            assert!(reloaded.can_open_container(&key, false));
        }
    }
}

#[tokio::test]
async fn unsupported_container_metadata_edits_are_typed_atomic_refusals() {
    use crate::block::entities::{BlockEntityLoadError, NbtEditDecision};
    for kind in CONTAINERS {
        let fixture = Fixture::new().await;
        let live = kind.load(
            &compound(
                "{CustomName:'Keep',LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}",
            ),
            BLOCK_POS,
        );
        fixture.install(kind.block(), live.clone());
        let before = saved(live.as_ref());
        let token = kind.token(live.as_ref());
        live.clear_dirty();
        for candidate in [
            "{lock:{items:'minecraft:stick'}}",
            "{lock:{count:2}}",
            // Java returns a partial retaining the name predicate here. This
            // bounded port refuses the known-field mix instead of unlocking it.
            "{lock:{count:bad,components:{'minecraft:custom_name':'key'}}}",
            "{lock:{predicates:{'minecraft:custom_data':{key:1}}}}",
            "{lock:{components:{'minecraft:custom_name':{text:'key',bold:1b}}}}",
            "{CustomName:{translate:'container.chest'}}",
            "{components:1}",
        ] {
            let update = compound(candidate);
            let mut published = false;
            let result = live.edit_nbt(
                &mut |nbt| {
                    for (key, value) in &update.child_tags {
                        nbt.put(key, value.clone());
                    }
                    nbt.put_long("LootTableSeed", 99);
                    NbtEditDecision::Commit
                },
                &mut |_| published = true,
            );
            assert!(matches!(result, Err(BlockEntityLoadError::Unsupported(_))));
            assert!(!published);
            assert!(!live.is_dirty());
            assert!(Arc::ptr_eq(&token, &kind.token(live.as_ref())));
            assert_eq!(saved(live.as_ref()), before);
            assert_eq!(fixture.pending(BLOCK_POS), before);
            assert!(
                fixture
                    .dispatcher
                    .execute_input(
                        &format!("data merge block 0 64 0 {candidate}"),
                        &fixture.source
                    )
                    .is_err()
            );
            assert_eq!(saved(live.as_ref()), before);
        }
        let mut replacement = before.clone();
        replacement.put("lock", compound("{items:'minecraft:stick'}"));
        assert!(matches!(
            live.load_nbt(&replacement),
            Err(BlockEntityLoadError::Unsupported(_))
        ));
        assert_eq!(saved(live.as_ref()), before);
        assert!(Arc::ptr_eq(&token, &kind.token(live.as_ref())));
    }
}

#[test]
fn container_load_replaces_metadata_without_replacing_retained_inventory_or_menu() {
    for kind in CONTAINERS {
        let live = kind.load(&compound("{CustomName:'Old',lock:{components:{'minecraft:custom_name':'old key'}},components:{'minecraft:custom_data':{keep:1}}}"), BLOCK_POS);
        let inventory = live.clone().get_inventory().unwrap();
        let player = MenuPlayer::new();
        let menu = create_generic_9x3(1, &player.0, inventory.clone(), &player);
        let old_token = kind.token(live.as_ref());
        let replacement = compound(
            "{CustomName:{text:'New',italic:0b},lock:{components:{'minecraft:custom_name':'new key'}},components:{'minecraft:custom_data':{keep:2}},Items:[{Slot:0b,id:'minecraft:apple',count:3}]}",
        );
        live.load_nbt(&replacement).unwrap();
        assert!(Arc::ptr_eq(
            &inventory,
            &live.clone().get_inventory().unwrap()
        ));
        assert!(!Arc::ptr_eq(&old_token, &kind.token(live.as_ref())));
        assert_eq!(menu.get_behaviour().slots[0].get_stack().item_count, 3);
        menu.get_behaviour().slots[1].set_stack(ItemStack::new(2, &Item::DIAMOND));
        for field in ["CustomName", "lock", "components"] {
            assert_eq!(saved(live.as_ref()).get(field), replacement.get(field));
        }
        assert_eq!(live.container_custom_name().unwrap().get_text(), "New");
        assert_eq!(
            live.container_custom_name().unwrap().0.style.italic,
            Some(false)
        );
        let mut key = ItemStack::new(1, &Item::STICK);
        key.set_custom_name("old key".into());
        assert!(!live.can_open_container(&key, false));
        key.set_custom_name("new key".into());
        assert!(live.can_open_container(&key, false));
    }
}

/// Observe without ticking: menu callbacks must already have published effects.
fn assert_viewer_effects(kind: ChestKind, world: &Arc<World>, live: &dyn BlockEntity, count: u8) {
    assert_eq!(kind.viewers(live), u16::from(count));
    let position = live.get_position();
    let state = world.get_block_state(&position);
    match kind {
        ChestKind::Barrel => {
            assert_eq!(
                BarrelLikeProperties::from_state_id(state.id).open,
                count > 0
            );
        }
        ChestKind::Trapped => {
            assert_eq!(
                TrappedChestBlock.get_weak_redstone_power(GetRedstonePowerArgs {
                    world,
                    block: kind.block(),
                    state,
                    position: &position,
                    direction: BlockDirection::North,
                }),
                count
            );
            assert_eq!(
                TrappedChestBlock.get_strong_redstone_power(GetRedstonePowerArgs {
                    world,
                    block: kind.block(),
                    state,
                    position: &position,
                    direction: BlockDirection::Up,
                }),
                count
            );
        }
        ChestKind::Normal => {}
    }
}

// Reuse the stopped-worker server fixture. Player::new needs a live Server;
// no packet reader/writer tasks or game loop are started. The loopback socket
// only supplies the real JavaClient transport required by Player's constructor.
struct MenuFixture(crate::command::commands::data::tests::Fixture);

impl MenuFixture {
    async fn new() -> Self {
        Self(crate::command::commands::data::tests::Fixture::new().await)
    }

    async fn with_registry(registry: Arc<crate::block::registry::BlockRegistry>) -> Self {
        let mut fixture = Self::new().await;
        let original = fixture.source.world();
        // Reuse the stopped Full-chunk level and live server required by Player;
        // this World only substitutes a registry that records real callbacks.
        let world = Arc::new(World::load(
            original.level.clone(),
            original.level_info.clone(),
            Dimension::OVERWORLD,
            registry,
            original.server.clone(),
            original.scoreboard.clone(),
        ));
        fixture.0.source = CommandSource {
            server: fixture.source.server.clone(),
            ..CommandSource::dummy().with_world(world).with_silent()
        };
        fixture
    }
}

impl std::ops::Deref for MenuFixture {
    type Target = crate::command::commands::data::tests::Fixture;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for MenuFixture {
    fn drop(&mut self) {
        // Player owns its World; break the registration cycle before teardown.
        self.source.world().players.store(Arc::new(Vec::new()));
    }
}

async fn registered_menu_player(world: &Arc<World>, mode: GameMode) -> Arc<Player> {
    use crate::net::{
        ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
        java::{JavaClient, pending::PendingConnection},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (peer, accepted) =
        tokio::join!(tokio::net::TcpStream::connect(address), listener.accept(),);
    let _peer = peer.unwrap();
    let (socket, address) = accepted.unwrap();
    let profile = GameProfile {
        id: uuid::Uuid::new_v4(),
        name: "ContainerViewer".into(),
        properties: ArcSwap::from_pointee(Vec::new()),
        profile_actions: None,
    };
    let pending = PendingConnection::new(
        socket,
        address,
        0,
        PacketRateLimiter::new(false, 0.0, 0.0),
        world.server.clone(),
    );
    let client = Arc::new(ClientPlatform::Java(JavaClient::from_pending(
        pending,
        profile.clone(),
        PlayerConfig::default(),
    )));
    let player = Arc::new(Player::new(
        client,
        profile,
        PlayerConfig::default(),
        world,
        mode,
    ));
    player
        .living_entity
        .entity
        .set_pos(BLOCK_POS.to_centered_f64());
    world.players.rcu(|players| {
        let mut players = players.as_ref().clone();
        players.push(player.clone());
        Arc::new(players)
    });
    player
}

fn open_menu(
    player: &Player,
    factory: &dyn ScreenHandlerFactory,
    position: BlockPos,
) -> SharedScreenHandler {
    assert!(
        player
            .open_handled_screen(factory, Some(position))
            .is_some()
    );
    player.current_screen_handler.lock().unwrap().clone()
}

fn recheck_viewers(world: &Arc<World>, entities: &[&dyn BlockEntity]) {
    for _ in 0..5 {
        for entity in entities {
            entity.tick(world);
        }
    }
}

#[tokio::test]
async fn materialized_container_factory_excludes_spectators_from_viewer_effects() {
    // Official 26.3 Chest/Barrel startOpen and stopOpen skip spectators;
    // TrappedChest inherits Chest. The pinned pumpkin-inventory 5b53f064
    // constructor (lines 166–168) and on_closed (207–211) already guard both.
    // Test the core factory path and immediate effects, without any BE tick.
    for kind in CONTAINERS {
        let observed = Arc::new(Observed::default());
        let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
        let world = fixture.source.world();
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let inventory = live.clone().get_inventory().unwrap();
        let factory = kind.factory(world, &BLOCK_POS);
        let spectator = registered_menu_player(world, GameMode::Spectator).await;
        live.load_nbt(&compound(
            "{LootTable:'minecraft:chests/simple_dungeon',LootTableSeed:73L}",
        ))
        .unwrap();
        let pending = saved(live.as_ref());
        assert!(
            spectator
                .open_handled_screen(factory.as_ref(), Some(BLOCK_POS))
                .is_none()
        );
        assert_eq!(saved(live.as_ref()), pending);
        assert_viewer_effects(kind, world, live.as_ref(), 0);
        world.flush_synced_block_events();
        assert!(observed.take_events().is_empty());
        assert!(observed.take_neighbors().is_empty());
        live.load_nbt(&NbtCompound::new()).unwrap();
        let spectator_menu = open_menu(&spectator, factory.as_ref(), BLOCK_POS);
        assert_viewer_effects(kind, world, live.as_ref(), 0);
        world.flush_synced_block_events();
        assert!(observed.take_events().is_empty());
        assert!(std::ptr::eq(
            inventory.as_any(),
            spectator_menu.lock().unwrap().get_behaviour().slots[0]
                .get_inventory()
                .as_any()
        ));
        assert_eq!(fixture.execute("data modify block 0 64 0 Items set value [{Slot:0b,id:'minecraft:diamond',count:3}]" ).unwrap(), 1);
        assert_eq!(
            spectator_menu.lock().unwrap().get_behaviour().slots[0]
                .get_stack()
                .item_count,
            3
        );
        assert!(Arc::ptr_eq(
            &live,
            &world.get_block_entity(&BLOCK_POS).unwrap()
        ));
        spectator.on_handled_screen_closed();
        assert_viewer_effects(kind, world, live.as_ref(), 0);
        world.flush_synced_block_events();
        assert!(observed.take_events().is_empty());

        let normal = registered_menu_player(world, GameMode::Survival).await;
        for spectator_closes_first in [true, false] {
            let _normal_menu = open_menu(&normal, factory.as_ref(), BLOCK_POS);
            assert_viewer_effects(kind, world, live.as_ref(), 1);
            let _spectator_menu = open_menu(&spectator, factory.as_ref(), BLOCK_POS);
            assert_viewer_effects(kind, world, live.as_ref(), 1);
            if spectator_closes_first {
                // A spectator menu must not steal another viewer's count, even
                // if its owner changes mode before the menu is closed.
                spectator.gamemode.store(GameMode::Survival);
                spectator.on_handled_screen_closed();
                spectator.gamemode.store(GameMode::Spectator);
                assert_viewer_effects(kind, world, live.as_ref(), 1);
                normal.on_handled_screen_closed();
            } else {
                normal.on_handled_screen_closed();
                assert_viewer_effects(kind, world, live.as_ref(), 0);
                spectator.on_handled_screen_closed();
            }
            assert_viewer_effects(kind, world, live.as_ref(), 0);
        }
    }
}

#[tokio::test]
async fn double_chest_factory_excludes_spectators_on_both_halves() {
    for kind in CHESTS {
        let observed = Arc::new(Observed::default());
        let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
        let world = fixture.source.world();
        let first = kind.create(BLOCK_POS);
        let second = kind.create(OTHER_POS);
        fixture.install(kind.block(), first.clone());
        fixture.install(kind.block(), second.clone());
        let mut properties = ChestLikeProperties::from_state_id(kind.block().default_state.id);
        properties.facing = HorizontalFacing::South;
        properties.r#type = ChestType::Right;
        fixture.set_state(BLOCK_POS, properties.to_state_id(kind.block()));
        properties.r#type = ChestType::Left;
        fixture.set_state(OTHER_POS, properties.to_state_id(kind.block()));
        let spectator = registered_menu_player(world, GameMode::Spectator).await;
        let normal = registered_menu_player(world, GameMode::Survival).await;
        for clicked in [BLOCK_POS, OTHER_POS] {
            let factory = kind.factory(world, &clicked);
            let spectator_menu = open_menu(&spectator, factory.as_ref(), clicked);
            for entity in [&first, &second] {
                assert_viewer_effects(kind, world, entity.as_ref(), 0);
            }
            world.flush_synced_block_events();
            assert!(observed.take_events().is_empty());
            first
                .clone()
                .get_inventory()
                .unwrap()
                .set_stack(0, ItemStack::new(2, &Item::APPLE));
            second
                .clone()
                .get_inventory()
                .unwrap()
                .set_stack(0, ItemStack::new(5, &Item::DIAMOND));
            {
                let menu = spectator_menu.lock().unwrap();
                assert_eq!(menu.get_behaviour().slots[0].get_stack().item_count, 2);
                assert_eq!(menu.get_behaviour().slots[27].get_stack().item_count, 5);
            };
            let _normal_menu = open_menu(&normal, factory.as_ref(), clicked);
            for entity in [&first, &second] {
                assert_viewer_effects(kind, world, entity.as_ref(), 1);
            }
            world.flush_synced_block_events();
            assert_eq!(
                observed.take_events(),
                [(BLOCK_POS, 1, 1), (OTHER_POS, 1, 1)]
            );
            normal.on_handled_screen_closed();
            for entity in [&first, &second] {
                assert_viewer_effects(kind, world, entity.as_ref(), 0);
            }
            world.flush_synced_block_events();
            assert_eq!(
                observed.take_events(),
                [(BLOCK_POS, 1, 0), (OTHER_POS, 1, 0)]
            );
            spectator.on_handled_screen_closed();
            world.flush_synced_block_events();
            assert!(observed.take_events().is_empty());
            for entity in [&first, &second] {
                assert_viewer_effects(kind, world, entity.as_ref(), 0);
                assert!(Arc::ptr_eq(
                    entity,
                    &world.get_block_entity(&entity.get_position()).unwrap()
                ));
            }
        }
    }
}

#[tokio::test]
async fn normal_opener_becoming_spectator_is_recounted_after_five_ticks() {
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let player = registered_menu_player(world, GameMode::Survival).await;
    for kind in CONTAINERS {
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        let factory = kind.factory(world, &BLOCK_POS);
        player.gamemode.store(GameMode::Survival);
        let retained = open_menu(&player, factory.as_ref(), BLOCK_POS);
        recheck_viewers(world, &[live.as_ref()]);
        assert_eq!(
            kind.viewers(live.as_ref()),
            1,
            "normal menu survives recount"
        );
        let inventory = live.clone().get_inventory().unwrap();
        assert!(std::ptr::eq(
            inventory.as_any(),
            retained.lock().unwrap().get_behaviour().slots[0]
                .get_inventory()
                .as_any()
        ));

        player.gamemode.store(GameMode::Spectator);
        player.on_handled_screen_closed();
        // The upstream stop guard skips this decrement; the BE recount repairs it.
        for _ in 0..4 {
            live.tick(world);
            assert_eq!(kind.viewers(live.as_ref()), 1);
        }
        live.tick(world);
        assert_viewer_effects(kind, world, live.as_ref(), 0);

        // A normal close still takes effect immediately, without waiting five ticks.
        player.gamemode.store(GameMode::Survival);
        open_menu(&player, factory.as_ref(), BLOCK_POS);
        assert_viewer_effects(kind, world, live.as_ref(), 1);
        player.on_handled_screen_closed();
        assert_viewer_effects(kind, world, live.as_ref(), 0);
    }
}

#[tokio::test]
async fn double_recount_uses_both_retained_half_identities_after_mode_change_and_replacement() {
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let player = registered_menu_player(world, GameMode::Survival).await;
    for kind in CHESTS {
        for clicked in [BLOCK_POS, OTHER_POS] {
            let first = kind.create(BLOCK_POS);
            let second = kind.create(OTHER_POS);
            fixture.install(kind.block(), first.clone());
            fixture.install(kind.block(), second.clone());
            let mut properties = ChestLikeProperties::from_state_id(kind.block().default_state.id);
            properties.facing = HorizontalFacing::South;
            properties.r#type = ChestType::Right;
            fixture.set_state(BLOCK_POS, properties.to_state_id(kind.block()));
            properties.r#type = ChestType::Left;
            fixture.set_state(OTHER_POS, properties.to_state_id(kind.block()));
            player.gamemode.store(GameMode::Survival);
            let factory = kind.factory(world, &clicked);
            let retained = open_menu(&player, factory.as_ref(), clicked);
            recheck_viewers(world, &[first.as_ref(), second.as_ref()]);
            for entity in [&first, &second] {
                assert_eq!(kind.viewers(entity.as_ref()), 1);
            }

            // A replacement at the same coordinates is not owned by this menu.
            let replacement = kind.create(BLOCK_POS);
            world.add_block_entity(replacement.clone());
            let replacement_inventory = replacement.clone().get_inventory().unwrap();
            replacement_inventory.on_open();
            recheck_viewers(
                world,
                &[replacement.as_ref(), first.as_ref(), second.as_ref()],
            );
            assert_eq!(kind.viewers(replacement.as_ref()), 0);
            assert_eq!(kind.viewers(first.as_ref()), 1);
            assert_eq!(kind.viewers(second.as_ref()), 1);
            retained.lock().unwrap().get_behaviour().slots[27]
                .set_stack(ItemStack::new(2, &Item::APPLE));
            assert_eq!(
                second
                    .clone()
                    .get_inventory()
                    .unwrap()
                    .get_stack(0)
                    .item_count,
                2
            );

            player.gamemode.store(GameMode::Spectator);
            player.on_handled_screen_closed();
            recheck_viewers(world, &[first.as_ref(), second.as_ref()]);
            assert_viewer_effects(kind, world, first.as_ref(), 0);
            assert_viewer_effects(kind, world, second.as_ref(), 0);
        }
    }
}

struct PausingFactory {
    inner: Box<dyn ScreenHandlerFactory>,
    created: mpsc::Sender<()>,
    resume: Mutex<mpsc::Receiver<()>>,
}

impl ScreenHandlerFactory for PausingFactory {
    fn create_screen_handler(
        &self,
        id: u8,
        inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let menu = self.inner.create_screen_handler(id, inventory, player)?;
        assert!(
            player
                .as_any()
                .downcast_ref::<Player>()
                .unwrap()
                .menu_publication
                .is_active()
        );
        self.created.send(()).unwrap();
        self.resume
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        Some(menu)
    }

    fn get_display_name(&self) -> pumpkin_util::text::TextComponent {
        self.inner.get_display_name()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recount_defers_between_factory_on_open_and_player_menu_publication() {
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let old = registered_menu_player(world, GameMode::Survival).await;
    let next = registered_menu_player(world, GameMode::Survival).await;
    let live = ChestKind::Barrel.create(BLOCK_POS);
    fixture.install(&Block::BARREL, live.clone());
    let factory = ChestKind::Barrel.factory(world, &BLOCK_POS);
    open_menu(&old, factory.as_ref(), BLOCK_POS);
    recheck_viewers(world, &[live.as_ref()]);
    let (created, wait_created) = mpsc::channel();
    let (resume, wait_resume) = mpsc::channel();
    let factory = PausingFactory {
        inner: factory,
        created,
        resume: Mutex::new(wait_resume),
    };
    let runtime = tokio::runtime::Handle::current();
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| {
            let _runtime = runtime.enter();
            open_menu(&next, &factory, BLOCK_POS)
        });
        wait_created.recv_timeout(Duration::from_secs(5)).unwrap();
        old.gamemode.store(GameMode::Spectator);
        old.on_handled_screen_closed();
        // A nested transition ending cannot clear the pending outer publication.
        drop(next.menu_publication.begin());
        assert!(next.menu_publication.is_active());
        recheck_viewers(world, &[live.as_ref()]);
        assert_viewer_effects(ChestKind::Barrel, world, live.as_ref(), 2);
        resume.send(()).unwrap();
        pending.join().unwrap();
    });
    assert!(!next.menu_publication.is_active());
    recheck_viewers(world, &[live.as_ref()]);
    assert_viewer_effects(ChestKind::Barrel, world, live.as_ref(), 1);
    next.on_handled_screen_closed();
    assert_viewer_effects(ChestKind::Barrel, world, live.as_ref(), 0);
}

struct MenuEventHook<E>(Box<dyn Fn(&mut E) + Send + Sync>);

impl<E: crate::plugin::api::events::Payload + Send + Sync> crate::plugin::EventHandler<E>
    for MenuEventHook<E>
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<crate::server::Server>,
        event: &'a mut E,
    ) -> crate::plugin::BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recount_defers_during_close_callback_and_cancelled_open_releases_guard() {
    use crate::plugin::api::events::{
        EventPriority, inventory::inventory_open::InventoryOpenEvent,
        player::inventory_close::InventoryCloseEvent,
    };
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let server = world.server.upgrade().unwrap();
    let first = registered_menu_player(world, GameMode::Survival).await;
    let second = registered_menu_player(world, GameMode::Survival).await;
    let live = ChestKind::Trapped.create(BLOCK_POS);
    fixture.install(&Block::TRAPPED_CHEST, live.clone());
    let factory = ChestKind::Trapped.factory(world, &BLOCK_POS);
    open_menu(&first, factory.as_ref(), BLOCK_POS);
    open_menu(&second, factory.as_ref(), BLOCK_POS);
    recheck_viewers(world, &[live.as_ref()]);
    assert_eq!(ChestKind::Trapped.viewers(live.as_ref()), 2);
    let (closing, wait_closing) = mpsc::channel();
    let (resume, wait_resume) = mpsc::channel();
    let wait_resume = Mutex::new(wait_resume);
    let first_id = first.gameprofile.id;
    server.plugin_manager.register::<InventoryCloseEvent, _>(
        Arc::new(MenuEventHook(Box::new(
            move |event: &mut InventoryCloseEvent| {
                if event.player.gameprofile.id == first_id {
                    assert!(event.player.menu_publication.is_active());
                    closing.send(()).unwrap();
                    wait_resume
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
            },
        ))),
        EventPriority::Normal,
        true,
    );
    let runtime = tokio::runtime::Handle::current();
    std::thread::scope(|scope| {
        let pending = scope.spawn(|| {
            let _runtime = runtime.enter();
            first.on_handled_screen_closed();
        });
        wait_closing.recv_timeout(Duration::from_secs(5)).unwrap();
        recheck_viewers(world, &[live.as_ref()]);
        // The closed menu is still published, but may not be counted back in.
        assert_viewer_effects(ChestKind::Trapped, world, live.as_ref(), 1);
        resume.send(()).unwrap();
        pending.join().unwrap();
    });
    assert!(!first.menu_publication.is_active());
    recheck_viewers(world, &[live.as_ref()]);
    assert_viewer_effects(ChestKind::Trapped, world, live.as_ref(), 1);
    server.plugin_manager.register::<InventoryOpenEvent, _>(
        Arc::new(MenuEventHook(Box::new(|event: &mut InventoryOpenEvent| {
            assert!(event.player.menu_publication.is_active());
            event.cancelled = true;
        }))),
        EventPriority::Normal,
        true,
    );
    assert!(
        first
            .open_handled_screen(factory.as_ref(), Some(BLOCK_POS))
            .is_none()
    );
    assert!(!first.menu_publication.is_active());
    assert_eq!(ChestKind::Trapped.viewers(live.as_ref()), 1);
    second.on_handled_screen_closed();
    assert_viewer_effects(ChestKind::Trapped, world, live.as_ref(), 0);
}

#[tokio::test]
async fn recount_revision_rejects_open_close_aba_before_menu_scan() {
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let old = registered_menu_player(world, GameMode::Survival).await;
    let next = registered_menu_player(world, GameMode::Survival).await;
    let live = Arc::new(ChestBlockEntity::new(BLOCK_POS));
    fixture.install(&Block::CHEST, live.clone());
    let factory = ChestKind::Normal.factory(world, &BLOCK_POS);
    open_menu(&old, factory.as_ref(), BLOCK_POS);
    recheck_viewers(world, &[live.as_ref()]);
    old.gamemode.store(GameMode::Spectator);
    old.on_handled_screen_closed();
    live.viewers.before_recount(move || {
        open_menu(&next, factory.as_ref(), BLOCK_POS);
        next.on_handled_screen_closed();
    });
    recheck_viewers(world, &[live.as_ref()]);
    assert_eq!(
        live.get_viewer_count(),
        1,
        "same numeric count has a new revision"
    );
    recheck_viewers(world, &[live.as_ref()]);
    assert_viewer_effects(ChestKind::Normal, world, live.as_ref(), 0);
}

#[derive(Default)]
struct Observed {
    events: Mutex<Vec<(BlockPos, u8, u8)>>,
    neighbors: Mutex<Vec<BlockPos>>,
    on_neighbor: Mutex<Option<ViewerEffectHook>>,
}
type ViewerEffectHook = Box<dyn FnOnce(&Arc<World>) + Send + Sync>;
impl Observed {
    fn registry(self: &Arc<Self>, kind: ChestKind) -> Arc<crate::block::registry::BlockRegistry> {
        let mut registry = default_registry();
        Arc::get_mut(&mut registry)
            .unwrap()
            .register(RecordEvents(self.clone()));
        Arc::get_mut(&mut registry)
            .unwrap()
            .register(RecordNeighbors(kind, self.clone()));
        registry
    }
    fn take_events(&self) -> Vec<(BlockPos, u8, u8)> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
    fn take_neighbors(&self) -> Vec<BlockPos> {
        std::mem::take(&mut *self.neighbors.lock().unwrap())
    }
}
struct RecordEvents(Arc<Observed>);
impl BlockMetadata for RecordEvents {
    fn ids() -> Box<[BlockId]> {
        [BlockId::CHEST, BlockId::TRAPPED_CHEST, BlockId::BARREL].into()
    }
}
impl BlockBehaviour for RecordEvents {
    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        self.0
            .events
            .lock()
            .unwrap()
            .push((*args.position, args.r#type, args.data));
        // Observe the actual queue flush, then retain the production decision
        // about forwarding the event. The BE/listener is never substituted.
        match args.block.id {
            BlockId::CHEST => ChestBlock.on_synced_block_event(args),
            BlockId::TRAPPED_CHEST => TrappedChestBlock.on_synced_block_event(args),
            BlockId::BARREL => BarrelBlock.on_synced_block_event(args),
            _ => panic!("recording fixture received an unexpected container block"),
        }
    }
}
struct RecordNeighbors(ChestKind, Arc<Observed>);
impl BlockMetadata for RecordNeighbors {
    fn ids() -> Box<[BlockId]> {
        [BlockId::COMPARATOR].into()
    }
}
impl BlockBehaviour for RecordNeighbors {
    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let live = args.world.get_block_entity(&BLOCK_POS).unwrap();
        // Reenter the real tracker to catch callbacks under its state lock.
        let _count = self.0.viewers(live.as_ref());
        assert!(!self.0.state_is_locked(live.as_ref()));
        self.1.neighbors.lock().unwrap().push(*args.position);
        let hook = self.1.on_neighbor.lock().unwrap().take();
        if let Some(hook) = hook {
            hook(args.world);
        }
    }
}

#[tokio::test]
async fn accepted_recounts_resend_chest_lids_without_repeating_neighbor_updates() {
    for kind in CONTAINERS {
        let observed = Arc::new(Observed::default());
        let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
        let world = fixture.source.world();
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        for position in [OTHER_POS, OTHER_POS.down()] {
            fixture.set_state(position, Block::COMPARATOR.default_state.id);
        }
        let expected_events = |count| {
            if matches!(kind, ChestKind::Barrel) {
                Vec::new()
            } else {
                vec![(BLOCK_POS, 1, count)]
            }
        };
        let player = registered_menu_player(world, GameMode::Survival).await;
        let factory = kind.factory(world, &BLOCK_POS);
        open_menu(&player, factory.as_ref(), BLOCK_POS);
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(1));
        assert_eq!(kind.viewers(live.as_ref()), 1);
        let neighbors = observed.take_neighbors();
        if matches!(kind, ChestKind::Trapped) {
            assert_eq!(neighbors, [OTHER_POS, OTHER_POS.down()]);
        }
        if matches!(kind, ChestKind::Barrel) {
            assert!(BarrelLikeProperties::from_state_id(world.get_block_state_id(&BLOCK_POS)).open);
        }

        // No repeat on ticks 1–4. Exactly one on tick 5, then on each accepted
        // five-tick recheck. Neither unchanged recount updates neighbors.
        for _ in 0..4 {
            live.tick(world);
            world.flush_synced_block_events();
            assert!(observed.take_events().is_empty());
        }
        live.tick(world);
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(1));
        assert!(observed.take_neighbors().is_empty());
        recheck_viewers(world, &[live.as_ref()]);
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(1));
        assert!(observed.take_neighbors().is_empty());

        let publication = player.menu_publication.begin();
        recheck_viewers(world, &[live.as_ref()]);
        world.flush_synced_block_events();
        assert!(
            observed.take_events().is_empty(),
            "deferred scan is not a recount"
        );
        assert!(observed.take_neighbors().is_empty());
        drop(publication);
        if let Some(chest) = live.as_any().downcast_ref::<ChestBlockEntity>() {
            let another = registered_menu_player(world, GameMode::Survival).await;
            let another_factory = kind.factory(world, &BLOCK_POS);
            chest.viewers.before_recount(move || {
                open_menu(&another, another_factory.as_ref(), BLOCK_POS);
                another.on_handled_screen_closed();
            });
            recheck_viewers(world, &[live.as_ref()]);
            world.flush_synced_block_events();
            assert_eq!(
                observed.take_events(),
                [(BLOCK_POS, 1, 2), (BLOCK_POS, 1, 1)],
                "open and close publish immediately; the stale recount adds no event"
            );
            assert!(observed.take_neighbors().is_empty());
        }
        recheck_viewers(world, &[live.as_ref()]);
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(1));
        assert!(observed.take_neighbors().is_empty());

        player.on_handled_screen_closed();
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(0));
        assert_eq!(kind.viewers(live.as_ref()), 0);
        let neighbors = observed.take_neighbors();
        if matches!(kind, ChestKind::Trapped) {
            assert_eq!(neighbors, [OTHER_POS, OTHER_POS.down()]);
        }
        if matches!(kind, ChestKind::Barrel) {
            assert!(
                !BarrelLikeProperties::from_state_id(world.get_block_state_id(&BLOCK_POS)).open
            );
        }
        recheck_viewers(world, &[live.as_ref()]);
        world.flush_synced_block_events();
        assert_eq!(observed.take_events(), expected_events(0));
        assert!(observed.take_neighbors().is_empty());
        recheck_viewers(world, &[live.as_ref()]);
        world.flush_synced_block_events();
        assert!(
            observed.take_events().is_empty(),
            "inactive counters do not resend"
        );
        assert!(observed.take_neighbors().is_empty());
    }
}

#[tokio::test]
async fn final_recount_repairs_a_deduplicated_close_without_rearming_an_existing_tick() {
    for kind in CONTAINERS {
        let observed = Arc::new(Observed::default());
        let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
        let world = fixture.source.world();
        let live = kind.create(BLOCK_POS);
        fixture.install(kind.block(), live.clone());
        for position in [OTHER_POS, OTHER_POS.down()] {
            fixture.set_state(position, Block::COMPARATOR.default_state.id);
        }
        let player = registered_menu_player(world, GameMode::Survival).await;
        let factory = kind.factory(world, &BLOCK_POS);
        open_menu(&player, factory.as_ref(), BLOCK_POS);
        world.flush_synced_block_events();
        observed.take_events();
        observed.take_neighbors();

        // The first open already scheduled tick 5. A reopen at tick 2 must not
        // postpone it, even when the final count returns to zero before flush.
        for _ in 0..2 {
            live.tick(world);
        }
        player.on_handled_screen_closed();
        open_menu(&player, factory.as_ref(), BLOCK_POS);
        player.on_handled_screen_closed();
        assert_viewer_effects(kind, world, live.as_ref(), 0);
        world.flush_synced_block_events();
        let expected = if matches!(kind, ChestKind::Barrel) {
            Vec::new()
        } else {
            vec![(BLOCK_POS, 1, 0), (BLOCK_POS, 1, 1)]
        };
        assert_eq!(observed.take_events(), expected);
        let neighbors = observed.take_neighbors();
        if matches!(kind, ChestKind::Trapped) {
            assert_eq!(neighbors, [OTHER_POS, OTHER_POS.down()].repeat(3));
        } else {
            assert!(neighbors.is_empty());
        }

        for _ in 0..2 {
            live.tick(world);
            world.flush_synced_block_events();
            assert!(observed.take_events().is_empty());
        }
        live.tick(world);
        world.flush_synced_block_events();
        let expected = if matches!(kind, ChestKind::Barrel) {
            Vec::new()
        } else {
            vec![(BLOCK_POS, 1, 0)]
        };
        assert_eq!(observed.take_events(), expected);
        assert!(observed.take_neighbors().is_empty());
        for _ in 0..10 {
            live.tick(world);
            world.flush_synced_block_events();
            assert!(observed.take_events().is_empty());
        }
        assert!(observed.take_neighbors().is_empty());
    }
}

#[tokio::test]
async fn retained_menu_close_does_not_publish_effects_for_replacement_entity() {
    for kind in CONTAINERS {
        for double in [false, true] {
            if double && matches!(kind, ChestKind::Barrel) {
                continue;
            }
            let observed = Arc::new(Observed::default());
            let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
            let world = fixture.source.world();
            let original = kind.create(BLOCK_POS);
            fixture.install(kind.block(), original.clone());
            let second = double.then(|| {
                let second = kind.create(OTHER_POS);
                fixture.install(kind.block(), second.clone());
                let mut properties = ChestLikeProperties::default(kind.block());
                properties.facing = HorizontalFacing::South;
                properties.r#type = ChestType::Right;
                fixture.set_state(BLOCK_POS, properties.to_state_id(kind.block()));
                properties.r#type = ChestType::Left;
                fixture.set_state(OTHER_POS, properties.to_state_id(kind.block()));
                second
            });
            let old_player = registered_menu_player(world, GameMode::Survival).await;
            let new_player = registered_menu_player(world, GameMode::Survival).await;
            let old_factory = kind.factory(world, &BLOCK_POS);
            let retained = open_menu(&old_player, old_factory.as_ref(), BLOCK_POS);
            assert_viewer_effects(kind, world, original.as_ref(), 1);
            world.flush_synced_block_events();
            drop(observed.take_events());

            let replacement = kind.create(BLOCK_POS);
            world.add_block_entity(replacement.clone());
            let new_factory = kind.factory(world, &BLOCK_POS);
            open_menu(&new_player, new_factory.as_ref(), BLOCK_POS);
            assert_viewer_effects(kind, world, replacement.as_ref(), 1);
            world.flush_synced_block_events();
            drop(observed.take_events());

            // The old slot still mutates the retained allocation, not the new
            // entity at the same position. Its close must not close the new lid.
            retained.lock().unwrap().get_behaviour().slots[0]
                .set_stack(ItemStack::new(3, &Item::DIAMOND));
            assert_eq!(
                original
                    .clone()
                    .get_inventory()
                    .unwrap()
                    .get_stack(0)
                    .item_count,
                3
            );
            assert!(
                replacement
                    .clone()
                    .get_inventory()
                    .unwrap()
                    .get_stack(0)
                    .is_empty()
            );
            old_player.on_handled_screen_closed();
            assert_eq!(kind.viewers(original.as_ref()), 0);
            assert_viewer_effects(kind, world, replacement.as_ref(), 1);
            world.flush_synced_block_events();
            assert_eq!(
                observed.take_events(),
                if double {
                    vec![(OTHER_POS, 1, 1)]
                } else {
                    Vec::new()
                }
            );
            if let Some(second) = second {
                assert_viewer_effects(kind, world, second.as_ref(), 1);
            }
            new_player.on_handled_screen_closed();
            assert_viewer_effects(kind, world, replacement.as_ref(), 0);
        }
    }
}

#[tokio::test]
async fn barrel_menu_callbacks_preserve_facing_without_a_tick() {
    let fixture = MenuFixture::new().await;
    let world = fixture.source.world();
    let kind = ChestKind::Barrel;
    let live = kind.create(BLOCK_POS);
    fixture.install(kind.block(), live.clone());
    let mut properties = BarrelLikeProperties::from_state_id(Block::BARREL.default_state.id);
    properties.facing = Facing::East;
    properties.open = false;
    let closed = properties.to_state_id(&Block::BARREL);
    fixture.set_state(BLOCK_POS, closed);
    let player = registered_menu_player(world, GameMode::Survival).await;
    let factory = kind.factory(world, &BLOCK_POS);

    open_menu(&player, factory.as_ref(), BLOCK_POS);
    properties.open = true;
    assert_eq!(
        world.get_block_state_id(&BLOCK_POS),
        properties.to_state_id(&Block::BARREL)
    );
    assert_eq!(kind.viewers(live.as_ref()), 1);
    player.on_handled_screen_closed();
    assert_eq!(world.get_block_state_id(&BLOCK_POS), closed);
    assert_eq!(kind.viewers(live.as_ref()), 0);
    assert!(Arc::ptr_eq(
        &live,
        &world.get_block_entity(&BLOCK_POS).unwrap()
    ));
}

#[tokio::test]
async fn barrel_menu_callbacks_do_not_restore_replaced_blocks() {
    for replacement in [&Block::STONE, &Block::AIR] {
        for opening in [false, true] {
            let fixture = MenuFixture::new().await;
            let world = fixture.source.world();
            let kind = ChestKind::Barrel;
            let live = kind.create(BLOCK_POS);
            fixture.install(kind.block(), live.clone());
            let player = registered_menu_player(world, GameMode::Survival).await;
            // Retain the actual factory/menu before the replacement starts.
            let factory = kind.factory(world, &BLOCK_POS);
            if !opening {
                open_menu(&player, factory.as_ref(), BLOCK_POS);
                assert_viewer_effects(kind, world, live.as_ref(), 1);
            }

            // Deterministically stop in the replacement window: the chunk has
            // changed, but on_block_replaced/removal has not detached the BE.
            fixture.set_state(BLOCK_POS, replacement.default_state.id);
            assert!(Arc::ptr_eq(
                &live,
                &world.get_block_entity(&BLOCK_POS).unwrap()
            ));
            if opening {
                open_menu(&player, factory.as_ref(), BLOCK_POS);
            } else {
                player.on_handled_screen_closed();
            }
            assert_eq!(kind.viewers(live.as_ref()), u16::from(opening));
            assert_eq!(
                world.get_block_state_id(&BLOCK_POS),
                replacement.default_state.id
            );
            assert!(
                Arc::ptr_eq(&live, &world.get_block_entity(&BLOCK_POS).unwrap()),
                "the callback must not install a replacement block entity"
            );

            if opening {
                player.on_handled_screen_closed();
                assert_eq!(kind.viewers(live.as_ref()), 0);
                assert!(Arc::ptr_eq(
                    &live,
                    &world.get_block_entity(&BLOCK_POS).unwrap()
                ));
            }
            world.remove_block_entity(&BLOCK_POS);
            assert!(world.get_block_entity(&BLOCK_POS).is_none());
            assert_eq!(
                world.get_block_state_id(&BLOCK_POS),
                replacement.default_state.id
            );
        }
    }
}

#[tokio::test]
async fn viewer_drain_recovers_after_neighbor_panic_and_preserves_queued_order() {
    let kind = ChestKind::Trapped;
    let observed = Arc::new(Observed::default());
    let fixture = Fixture::with_registry(observed.registry(kind)).await;
    let world = fixture.source.world();
    let live = kind.create(BLOCK_POS);
    fixture.install(kind.block(), live.clone());
    for position in [OTHER_POS, OTHER_POS.down()] {
        fixture.set_state(position, Block::COMPARATOR.default_state.id);
    }
    // Use the actual menu inventory adapter without holding a menu mutex
    // across the injected panic. This isolates publisher unwind recovery.
    let inventory = live.clone().get_menu_inventory(world).unwrap();
    let nested = inventory.clone();
    *observed.on_neighbor.lock().unwrap() = Some(Box::new(move |_| {
        nested.on_open();
        nested.on_close();
        panic!("injected viewer neighbor panic");
    }));
    let panic = catch_unwind(AssertUnwindSafe(|| inventory.on_open())).unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"injected viewer neighbor panic")
    );
    assert_eq!(kind.viewers(live.as_ref()), 1);
    assert_eq!(observed.take_neighbors(), [OTHER_POS]);
    world.flush_synced_block_events();
    assert_eq!(observed.take_events(), [(BLOCK_POS, 1, 1)]);

    // The next callback must claim the abandoned drain and publish the older
    // queued transitions first, without a tick or replaying the partial update.
    inventory.on_close();
    assert_viewer_effects(kind, world, live.as_ref(), 0);
    assert_eq!(
        observed.take_neighbors(),
        [OTHER_POS, OTHER_POS.down()].repeat(3)
    );
    world.flush_synced_block_events();
    assert_eq!(
        observed.take_events(),
        [(BLOCK_POS, 1, 2), (BLOCK_POS, 1, 1), (BLOCK_POS, 1, 0)]
    );

    // Both directions still publish immediately after normal drain release.
    inventory.on_open();
    assert_viewer_effects(kind, world, live.as_ref(), 1);
    assert_eq!(observed.take_neighbors(), [OTHER_POS, OTHER_POS.down()]);
    world.flush_synced_block_events();
    assert_eq!(observed.take_events(), [(BLOCK_POS, 1, 1)]);
    inventory.on_close();
    assert_viewer_effects(kind, world, live.as_ref(), 0);
    assert_eq!(observed.take_neighbors(), [OTHER_POS, OTHER_POS.down()]);
    world.flush_synced_block_events();
    assert_eq!(observed.take_events(), [(BLOCK_POS, 1, 0)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn immediate_viewer_callbacks_allow_reentry_and_cancelled_open_has_no_effects() {
    use crate::plugin::{
        EventPriority, api::events::inventory::inventory_open::InventoryOpenEvent,
    };

    let kind = ChestKind::Trapped;
    let observed = Arc::new(Observed::default());
    let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
    let world = fixture.source.world();
    let live = kind.create(BLOCK_POS);
    fixture.install(kind.block(), live.clone());
    fixture.set_state(OTHER_POS, Block::COMPARATOR.default_state.id);
    fixture.set_state(OTHER_POS.down(), Block::COMPARATOR.default_state.id);
    let first = registered_menu_player(world, GameMode::Survival).await;
    let second = registered_menu_player(world, GameMode::Survival).await;
    let factory = kind.factory(world, &BLOCK_POS);
    let next_factory = kind.factory(world, &BLOCK_POS);
    let next = second.clone();
    *observed.on_neighbor.lock().unwrap() = Some(Box::new(move |world| {
        // A real world callback can read inventory state and enter another
        // menu transition. The outer tracker must hold neither state lock.
        let live = world.get_block_entity(&BLOCK_POS).unwrap();
        assert!(!kind.state_is_locked(live.as_ref()));
        assert_eq!(kind.viewers(live.as_ref()), 1);
        open_menu(&next, next_factory.as_ref(), BLOCK_POS);
        next.on_handled_screen_closed();
    }));
    let first_menu = open_menu(&first, factory.as_ref(), BLOCK_POS);
    assert_viewer_effects(kind, world, live.as_ref(), 1);
    world.flush_synced_block_events();
    // The pending event set coalesces the repeated count; all three count
    // transitions still notify neighbors before the flush.
    assert_eq!(
        observed.take_events(),
        [(BLOCK_POS, 1, 1), (BLOCK_POS, 1, 2)]
    );
    assert_eq!(observed.take_neighbors().len(), 6);

    let next_factory = kind.factory(world, &BLOCK_POS);
    *observed.on_neighbor.lock().unwrap() = Some(Box::new(move |world| {
        // Upstream on_closed invokes Inventory::on_close while holding this
        // menu. The callback must complete without attempting to relock it.
        assert!(first_menu.try_lock().is_err());
        let live = world.get_block_entity(&BLOCK_POS).unwrap();
        assert_eq!(kind.viewers(live.as_ref()), 0);
        assert!(!kind.state_is_locked(live.as_ref()));
        open_menu(&second, next_factory.as_ref(), BLOCK_POS);
        second.on_handled_screen_closed();
    }));
    first.on_handled_screen_closed();
    assert_viewer_effects(kind, world, live.as_ref(), 0);
    world.flush_synced_block_events();
    assert_eq!(
        observed.take_events(),
        [(BLOCK_POS, 1, 0), (BLOCK_POS, 1, 1)]
    );
    assert_eq!(observed.take_neighbors().len(), 6);

    let server = world.server.upgrade().unwrap();
    server.plugin_manager.register::<InventoryOpenEvent, _>(
        Arc::new(MenuEventHook(Box::new(|event: &mut InventoryOpenEvent| {
            assert!(event.player.menu_publication.is_active());
            event.cancelled = true;
        }))),
        EventPriority::Normal,
        true,
    );
    assert!(
        first
            .open_handled_screen(factory.as_ref(), Some(BLOCK_POS))
            .is_none()
    );
    assert!(!first.menu_publication.is_active());
    assert_viewer_effects(kind, world, live.as_ref(), 0);
    world.flush_synced_block_events();
    assert!(observed.take_events().is_empty());
    assert!(observed.take_neighbors().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_viewer_callbacks_drain_in_order_without_waiting_under_menu_locks() {
    let kind = ChestKind::Trapped;
    let observed = Arc::new(Observed::default());
    let fixture = MenuFixture::with_registry(observed.registry(kind)).await;
    let world = fixture.source.world();
    let live = kind.create(BLOCK_POS);
    fixture.install(kind.block(), live.clone());
    fixture.set_state(OTHER_POS, Block::COMPARATOR.default_state.id);
    let first = registered_menu_player(world, GameMode::Survival).await;
    let second = registered_menu_player(world, GameMode::Survival).await;
    let factory = kind.factory(world, &BLOCK_POS);
    let (entered, wait_entered) = mpsc::channel();
    let (resume, wait_resume) = mpsc::channel();
    let wait_resume = Mutex::new(wait_resume);
    *observed.on_neighbor.lock().unwrap() = Some(Box::new(move |_| {
        entered.send(()).unwrap();
        wait_resume
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }));
    let runtime = tokio::runtime::Handle::current();
    std::thread::scope(|scope| {
        let opening = scope.spawn(|| {
            let _runtime = runtime.enter();
            open_menu(&first, factory.as_ref(), BLOCK_POS);
        });
        wait_entered.recv_timeout(Duration::from_secs(5)).unwrap();
        open_menu(&second, factory.as_ref(), BLOCK_POS);
        second.on_handled_screen_closed();
        // Its transitions belong to the in-flight synchronous publisher. No
        // world tick is required and neither producer waits on that publisher.
        assert_eq!(kind.viewers(live.as_ref()), 1);
        resume.send(()).unwrap();
        opening.join().unwrap();
    });
    world.flush_synced_block_events();
    assert_eq!(
        observed.take_events(),
        [(BLOCK_POS, 1, 1), (BLOCK_POS, 1, 2)]
    );
    assert_viewer_effects(kind, world, live.as_ref(), 1);
    first.on_handled_screen_closed();
    world.flush_synced_block_events();
    assert_eq!(observed.take_events(), [(BLOCK_POS, 1, 0)]);
    assert_viewer_effects(kind, world, live.as_ref(), 0);
}
