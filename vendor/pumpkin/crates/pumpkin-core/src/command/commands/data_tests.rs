use super::*;
use crate::block::entities::barrel::BarrelBlockEntity;
use crate::block::entities::brushable_block::BrushableBlockBlockEntity;
use crate::block::entities::chest::ChestBlockEntity;
use crate::block::entities::comparator::ComparatorBlockEntity;
use crate::block::entities::hanging_sign::HangingSignBlockEntity;
use crate::block::entities::hopper::HopperBlockEntity;
use crate::block::entities::sign::{DyeColor, SignBlockEntity, SignEntityRef};
use crate::command::CommandSource;
use crate::command::argument_types::nbt_path::{
    ERROR_EXPECTED_LIST, ERROR_INVALID_INDEX, ERROR_NOTHING_FOUND,
};
use crate::command::errors::error_types::AnyCommandErrorType;
use crate::data::VanillaData;
use crate::entity::r#type::from_type;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::{
    Block, BlockStateId,
    block_properties::{BarrelLikeProperties, ComparatorLikeProperties, HorizontalFacing},
    entity::EntityType,
    item::Item,
    item_stack::ItemStack,
};
use pumpkin_inventory::{
    Inventory,
    entity_equipment::EntityEquipment,
    generic_container_screen_handler::create_generic_9x3,
    player::player_inventory::PlayerInventory,
    screen_handler::{InventoryPlayer, ScreenHandler},
};
use pumpkin_util::{math::vector2::Vector2, math::vector3::Vector3, world_seed::Seed};
use pumpkin_world::{
    chunk::ChunkData,
    chunk::{format::anvil::SingleChunkDataSerializer, io::Dirtiable},
    world_info::{LevelData, WorldInfoWriter, anvil::AnvilLevelInfo},
};
use std::sync::Mutex;

const BLOCK_POS: BlockPos = BlockPos::new(0, 64, 0);
const SOURCES: [&str; 3] = ["block 0 64 0", "entity @s", "storage repair:source"];

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(value) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected a compound fixture");
    };
    value
}

pub(crate) struct Fixture {
    pub(crate) source: CommandSource,
    pub(crate) dispatcher: CommandDispatcher,
    _folder: tempfile::TempDir,
}

impl Fixture {
    /// No listeners, ticks, external authentication, or cwd configuration loads.
    /// Stop chunk workers before manually loading the command's Full chunk.
    pub(crate) async fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        AnvilLevelInfo
            .write_world_info(&LevelData::default(Seed(0)), folder.path())
            .unwrap();
        let basic = BasicConfiguration {
            default_level_name: folder.path().to_string_lossy().into_owned(),
            seed: Seed(0),
            allow_nether: false,
            allow_end: false,
            allow_chat_reports: false,
            use_favicon: false,
            ..BasicConfiguration::default()
        };
        let mut advanced = AdvancedConfiguration::default();
        advanced.networking.java.online_mode = false;
        advanced.networking.bedrock.online_mode = false;
        let data = VanillaData {
            banned_ip_list: Default::default(),
            banned_player_list: Default::default(),
            operator_config: Default::default(),
            user_cache: Default::default(),
            whitelist_config: Default::default(),
        };
        let server = Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            data,
            Vec::new(),
        )
        .await
        .unwrap();
        server.finish_tasks().await;
        let world = server.worlds.load()[0].clone();
        world.level.shutdown().await.unwrap();
        world
            .level
            .loaded_chunks
            .insert(Vector2::new(0, 0), ChunkData::empty_sync(0, 0));
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                chunk.set_block_absolute_y(0, 64, 0, Block::CHEST.default_state.id);
            })
            .unwrap();
        let chest = Arc::new(ChestBlockEntity::new(BLOCK_POS));
        chest.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        world.add_block_entity(chest);
        let entity = from_type(
            &EntityType::PIG,
            Vector3::new(2.0, 64.0, 0.0),
            &world,
            uuid::Uuid::from_u128(1),
        );
        entity.read_nbt_non_mut(&compound("{Tags:[original]}"));
        let source = CommandSource {
            server: Some(server),
            ..CommandSource::dummy().with_world(world).with_entity(entity)
        };
        let mut dispatcher = CommandDispatcher::new();
        register(&mut dispatcher, &PermissionRegistry::default());
        crate::command::commands::execute::register(
            &mut dispatcher,
            &PermissionRegistry::default(),
        );
        let fixture = Self {
            source,
            dispatcher,
            _folder: folder,
        };
        fixture.put_storage(
            "repair:source",
            compound("{id:'minecraft:storage',numbers:[3,4],number:42,text:abcdef,object:{key:7}}"),
        );
        fixture
    }

    pub(crate) fn execute(&self, input: &str) -> CommandExecutorResult {
        self.dispatcher.execute_input(input, &self.source)
    }

    fn put_storage(&self, id: &str, data: NbtCompound) {
        StorageDataAccessor::new(id.into(), self.source.server().clone())
            .set_data(&data)
            .unwrap();
    }

    fn storage(&self, id: &str) -> NbtCompound {
        StorageDataAccessor::new(id.into(), self.source.server().clone())
            .get_data()
            .unwrap()
    }

    fn sources(&self) -> [NbtCompound; 3] {
        [
            BlockDataAccessor::new(BLOCK_POS, self.source.world().clone())
                .unwrap()
                .get_data()
                .unwrap(),
            EntityDataAccessor::new(self.source.entity.clone().unwrap())
                .get_data()
                .unwrap(),
            self.storage("repair:source"),
        ]
    }

    pub(crate) fn set_state(&self, position: BlockPos, state: BlockStateId) {
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

    pub(crate) fn install(&self, block: &Block, entity: Arc<dyn BlockEntity>) {
        self.set_state(entity.get_position(), block.default_state.id);
        self.source.world().add_block_entity(entity);
    }

    pub(crate) fn block_data(&self) -> NbtCompound {
        BlockDataAccessor::new(BLOCK_POS, self.source.world().clone())
            .unwrap()
            .get_data()
            .unwrap()
    }

    pub(crate) fn pending_data(&self) -> NbtCompound {
        self.source
            .world()
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                chunk
                    .pending_block_entities
                    .lock()
                    .unwrap()
                    .get(&BLOCK_POS)
                    .unwrap()
                    .clone()
            })
            .unwrap()
    }

    /// Round trip the actual terrain serializer, then use normal lazy BE loading.
    /// Caller retains the old Arc deliberately: the reloaded instance must be new.
    pub(crate) fn reload_saved_chunk(&self) -> Arc<dyn BlockEntity> {
        let world = self.source.world();
        let chunk_pos = BLOCK_POS.chunk_position();
        let bytes = world
            .level
            .read_chunk_sync(&chunk_pos, |chunk| chunk.to_bytes().unwrap())
            .unwrap();
        let reloaded = ChunkData::from_bytes(&bytes, chunk_pos).unwrap();
        world.block_entities.remove(&chunk_pos);
        world
            .level
            .loaded_chunks
            .insert(chunk_pos, Arc::new(reloaded));
        world.get_block_entity(&BLOCK_POS).unwrap()
    }
}

// Only the menu's player inventory is needed; no network client or game tick is started.
struct MenuPlayer(Arc<PlayerInventory>);

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
async fn barrel_data_mutates_the_open_menu_inventory_and_survives_terrain_reload() {
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
    let barrel = Arc::new(BarrelBlockEntity::new(BLOCK_POS));
    barrel.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
    barrel.set_stack(26, ItemStack::new(1, &Item::APPLE));
    fixture.install(&Block::BARREL, barrel.clone());
    let live: Arc<dyn BlockEntity> = barrel.clone();
    let inventory: Arc<dyn Inventory> = barrel.clone();
    let player = MenuPlayer(Arc::new(PlayerInventory::new(
        Arc::new(Mutex::new(EntityEquipment::new())),
        Arc::default(),
    )));
    let handler = create_generic_9x3(1, &player.0, inventory.clone(), &player);
    assert_eq!(handler.get_behaviour().slots[0].get_stack().item_count, 3);

    // Put a real comparator beside the barrel. The command must schedule its
    // output update without waiting for a later block-entity tick.
    let comparator_pos = BlockPos::new(1, 64, 0);
    fixture.set_state(comparator_pos.down(), Block::STONE.default_state.id);
    let mut comparator =
        ComparatorLikeProperties::from_state_id(Block::COMPARATOR.default_state.id);
    comparator.facing = HorizontalFacing::West;
    fixture.install(
        &Block::COMPARATOR,
        Arc::new(ComparatorBlockEntity::new(comparator_pos)),
    );
    fixture.set_state(comparator_pos, comparator.to_state_id(&Block::COMPARATOR));
    assert!(!world.is_block_tick_scheduled(&comparator_pos, &Block::COMPARATOR));
    world
        .level
        .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.mark_dirty(false))
        .unwrap();
    world
        .custom_block_entity_data
        .insert(BLOCK_POS, compound("{retained:9}"));

    assert_eq!(fixture.execute("data modify block 0 64 0 Items set value [{Slot:1b,id:'minecraft:emerald',count:32,components:{'minecraft:custom_data':{marker:17}}}]").unwrap(), 1);
    assert!(Arc::ptr_eq(
        &live,
        &world.get_block_entity(&BLOCK_POS).unwrap()
    ));
    assert!(Arc::ptr_eq(&inventory, &handler.inventory));
    assert!(handler.get_behaviour().slots[0].get_stack().is_empty());
    assert!(handler.get_behaviour().slots[26].get_stack().is_empty());
    let changed = handler.get_behaviour().slots[1].get_stack();
    assert_eq!(changed.item.id, Item::EMERALD.id);
    assert_eq!(changed.item_count, 32);
    let mut item_nbt = NbtCompound::new();
    changed.write_item_stack(&mut item_nbt);
    assert_eq!(
        item_nbt
            .get_compound("components")
            .unwrap()
            .get_compound("minecraft:custom_data"),
        Some(&compound("{marker:17}"))
    );
    assert!(barrel.is_dirty());
    assert!(
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| chunk.is_dirty())
            .unwrap()
    );
    assert!(world.is_block_tick_scheduled(&comparator_pos, &Block::COMPARATOR));
    assert_eq!(
        crate::block::calculate_comparator_output(barrel.as_ref()),
        1
    );
    assert_eq!(
        fixture
            .execute("data get block 0 64 0 Items[0].count")
            .unwrap(),
        32
    );
    assert_eq!(
        fixture.pending_data().get_compound("PumpkinCustomData"),
        Some(&compound("{retained:9}"))
    );
    assert_eq!(
        fixture.pending_data().get_list("Items"),
        fixture.block_data().get_list("Items")
    );

    world.flush_block_updates();
    assert!(
        world
            .take_flushed_block_changes()
            .contains(&(BLOCK_POS, Block::BARREL.default_state.id))
    );
    // Opening happened before reload. Its viewer tracker must still open the
    // barrel on the next tick, even though the item array has been replaced.
    barrel.tick(world);
    assert!(BarrelLikeProperties::from_state_id(world.get_block_state_id(&BLOCK_POS)).open);
    let reloaded = fixture.reload_saved_chunk();
    assert!(!Arc::ptr_eq(&live, &reloaded));
    let reloaded_inventory = reloaded.clone().get_inventory().unwrap();
    assert!(reloaded_inventory.get_stack(0).is_empty());
    assert!(reloaded_inventory.get_stack(1).are_equal(&changed));
    assert!(reloaded_inventory.get_stack(26).is_empty());
    // The old menu remains attached to its original instance even after this
    // explicit test-only unload/reload, proving no menu reference was substituted.
    assert!(
        handler.get_behaviour().slots[1]
            .get_stack()
            .are_equal(&changed)
    );
}

#[tokio::test]
async fn barrel_data_remove_items_clears_every_slot_and_ignores_spoofed_identity() {
    let fixture = Fixture::new().await;
    let barrel = Arc::new(BarrelBlockEntity::new(BLOCK_POS));
    barrel.set_stack(0, ItemStack::new(2, &Item::DIAMOND));
    barrel.set_stack(26, ItemStack::new(1, &Item::APPLE));
    fixture.install(&Block::BARREL, barrel.clone());
    assert_eq!(
        fixture
            .execute("data merge block 0 64 0 {id:'minecraft:chest',x:99,y:1,z:-8}")
            .unwrap(),
        1
    );
    let actual = fixture.block_data();
    assert_eq!(actual.get_string("id"), Some(BarrelBlockEntity::ID));
    assert_eq!(actual.get_int("x"), Some(0));
    assert_eq!(actual.get_int("y"), Some(64));
    assert_eq!(actual.get_int("z"), Some(0));
    assert_eq!(barrel.get_stack(0).item_count, 2);
    assert_eq!(barrel.get_stack(26).item_count, 1);
    assert_eq!(
        fixture.execute("data remove block 0 64 0 Items").unwrap(),
        1
    );
    assert!((0..BarrelBlockEntity::INVENTORY_SIZE).all(|slot| barrel.get_stack(slot).is_empty()));
    assert_eq!(
        crate::block::calculate_comparator_output(barrel.as_ref()),
        0
    );
    assert!(fixture.block_data().get_list("Items").unwrap().is_empty());
    assert!(fixture.pending_data().get_list("Items").unwrap().is_empty());
    let reloaded = fixture.reload_saved_chunk().get_inventory().unwrap();
    assert!(reloaded.is_empty());
}

#[tokio::test]
async fn sign_data_reloads_both_faces_wax_and_client_data_without_replacing_editor_handles() {
    let fixture = Fixture::new().await;
    let editor = uuid::Uuid::from_u128(97);
    for hanging in [false, true] {
        let (block, entity): (&Block, Arc<dyn BlockEntity>) = if hanging {
            (
                &Block::OAK_HANGING_SIGN,
                Arc::new(HangingSignBlockEntity::empty(BLOCK_POS)),
            )
        } else {
            (
                &Block::OAK_SIGN,
                Arc::new(SignBlockEntity::empty(BLOCK_POS)),
            )
        };
        fixture.install(block, entity.clone());
        let sign = SignEntityRef::from_block_entity(entity.as_ref()).unwrap();
        let editor_handle = sign.currently_editing_player().clone();
        *editor_handle.lock().unwrap() = Some(editor);
        let front_handle = sign.front_text().clone();
        let back_handle = sign.back_text().clone();
        assert_eq!(fixture.execute("data merge block 0 64 0 {front_text:{messages:['front','two','three','four'],filtered_messages:['filtered','','',''],color:red,has_glowing_text:1b},back_text:{messages:['back','','',''],color:blue,has_glowing_text:0b},is_waxed:1b,id:'minecraft:furnace',x:8,y:9,z:10}").unwrap(), 1);
        assert!(Arc::ptr_eq(
            &entity,
            &fixture.source.world().get_block_entity(&BLOCK_POS).unwrap()
        ));
        assert!(Arc::ptr_eq(&editor_handle, sign.currently_editing_player()));
        assert!(front_handle.shares_message_storage(sign.front_text(), false));
        assert!(back_handle.shares_message_storage(sign.back_text(), true));
        assert_eq!(*editor_handle.lock().unwrap(), Some(editor));
        assert_eq!(front_handle.get_message(0, false).as_ref(), "front");
        assert_eq!(back_handle.get_message(0, true).as_ref(), "back");
        assert_eq!(sign.front_text().get_message(0, true).as_ref(), "filtered");
        assert_eq!(sign.front_text().get_color(), DyeColor::Red);
        assert!(sign.front_text().has_glowing_text());
        assert_eq!(sign.back_text().get_color(), DyeColor::Blue);
        assert!(!sign.back_text().has_glowing_text());
        assert!(sign.is_waxed());
        let data = fixture.block_data();
        assert_eq!(data.get_string("id"), Some(entity.resource_location()));
        assert_eq!(data.get_int("x"), Some(0));
        assert_eq!(data.get_int("y"), Some(64));
        assert_eq!(data.get_int("z"), Some(0));
        let client = entity.chunk_data_nbt().unwrap();
        for key in ["front_text", "back_text", "is_waxed"] {
            assert_eq!(client.get(key), data.get(key));
            assert_eq!(fixture.pending_data().get(key), data.get(key));
        }
        assert_eq!(
            fixture
                .execute("data get block 0 64 0 front_text.messages[0]")
                .unwrap(),
            5
        );
        fixture.source.world().flush_block_updates();
        assert!(
            fixture
                .source
                .world()
                .take_flushed_block_changes()
                .contains(&(BLOCK_POS, block.default_state.id))
        );
        let reloaded = fixture.reload_saved_chunk();
        let reloaded_sign = SignEntityRef::from_block_entity(reloaded.as_ref()).unwrap();
        assert_eq!(reloaded.chunk_data_nbt(), Some(client));
        assert_eq!(
            *reloaded_sign.currently_editing_player().lock().unwrap(),
            None
        );
        assert_eq!(*editor_handle.lock().unwrap(), Some(editor));

        // Removing persisted fields follows constructor defaults, and the other
        // face survives an independent edit/removal on the front face.
        fixture
            .execute("data remove block 0 64 0 front_text")
            .unwrap();
        fixture
            .execute("data remove block 0 64 0 is_waxed")
            .unwrap();
        assert!(!reloaded_sign.front_text().has_message(false));
        assert_eq!(reloaded_sign.front_text().get_color(), DyeColor::Black);
        assert!(!reloaded_sign.front_text().has_glowing_text());
        assert_eq!(
            reloaded_sign.back_text().get_message(0, false).as_ref(),
            "back"
        );
        assert!(!reloaded_sign.is_waxed());
    }
}

#[tokio::test]
async fn unsupported_block_data_returns_an_explicit_error_without_staging_nbt() {
    let fixture = Fixture::new().await;
    fixture.install(
        &Block::HOPPER,
        Arc::new(HopperBlockEntity::from_nbt(
            &compound("{Items:[{Slot:0b,id:'minecraft:diamond',count:3}]}"),
            BLOCK_POS,
        )),
    );
    let live = fixture.source.world().get_block_entity(&BLOCK_POS).unwrap();
    let before = fixture.block_data();
    let pending = fixture.pending_data();
    for command in [
        "data modify block 0 64 0 Items set value []",
        "data merge block 0 64 0 {unimplemented:9}",
        "data remove block 0 64 0 Items",
    ] {
        let error = fixture.execute(command).unwrap_err();
        assert!(
            error.is(&ERROR_BLOCK_DATA_UNSUPPORTED),
            "{command}: {error:?}"
        );
        assert_eq!(fixture.block_data(), before);
        assert_eq!(fixture.pending_data(), pending);
        assert!(Arc::ptr_eq(
            &live,
            &fixture.source.world().get_block_entity(&BLOCK_POS).unwrap()
        ));
    }
}

#[tokio::test]
async fn captured_block_accessor_rejects_a_replacement_at_the_same_position() {
    let fixture = Fixture::new().await;
    let original = Arc::new(BarrelBlockEntity::new(BLOCK_POS));
    fixture.install(&Block::BARREL, original.clone());
    let accessor = BlockDataAccessor::new(BLOCK_POS, fixture.source.world().clone()).unwrap();
    let replacement = Arc::new(BarrelBlockEntity::new(BLOCK_POS));
    replacement.set_stack(0, ItemStack::new(5, &Item::APPLE));
    fixture.install(&Block::BARREL, replacement.clone());
    let pending = fixture.pending_data();
    let error = accessor
        .set_data(&compound(
            "{Items:[{Slot:0b,id:'minecraft:diamond',count:7}]}",
        ))
        .unwrap_err();
    assert!(error.is(&ERROR_BLOCK_INVALID));
    assert!(accessor.get_data().unwrap_err().is(&ERROR_BLOCK_INVALID));
    assert!(original.is_empty());
    assert_eq!(replacement.get_stack(0).item_count, 5);
    assert_eq!(fixture.pending_data(), pending);
}

#[tokio::test]
async fn block_entity_notification_saves_full_state_instead_of_its_limited_client_tag() {
    let fixture = Fixture::new().await;
    let entity: Arc<dyn BlockEntity> = Arc::new(BrushableBlockBlockEntity::from_nbt(
        &compound("{LootTable:'minecraft:archaeology/desert_pyramid',LootTableSeed:12L,hits:3}"),
        BLOCK_POS,
    ));
    fixture.install(&Block::SUSPICIOUS_SAND, entity.clone());
    let client = entity.chunk_data_nbt().unwrap();
    assert!(!client.has("LootTable"));
    assert!(!client.has("hits"));
    fixture.source.world().update_block_entity(&entity);
    let mut full = NbtCompound::new();
    entity.write_internal(&mut full);
    assert_eq!(fixture.pending_data(), full);
    let reloaded = fixture.reload_saved_chunk();
    let mut saved = NbtCompound::new();
    reloaded.write_internal(&mut saved);
    assert_eq!(saved, full);
}

#[tokio::test]
async fn modify_from_retains_every_provider_and_copies_without_mutating_sources() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let mut expected = NbtCompound::new();
    for (index, provider) in SOURCES.iter().enumerate() {
        let whole = format!("whole{index}");
        let id = format!("id{index}");
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target {whole} set from {provider}"
                ))
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target {id} set from {provider} id"
                ))
                .unwrap(),
            1
        );
        expected.put_compound(&whole, before[index].clone());
        expected.put_string(&id, before[index].get_string("id").unwrap().to_owned());
        assert_eq!(fixture.storage("repair:target"), expected);
    }

    // One list changes even when a source path selects multiple values; set uses
    // the final selected value. Neither operation consumes the source values.
    assert_eq!(
        fixture
            .execute("data modify storage repair:target copied append from storage repair:source numbers[]")
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target last set from storage repair:source numbers[]"
            )
            .unwrap(),
        1
    );
    expected.put("copied", NbtTag::List(vec![NbtTag::Int(3), NbtTag::Int(4)]));
    expected.put_int("last", 4);
    assert_eq!(fixture.storage("repair:target"), expected);
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_insert_requires_index_before_value_from_and_string_sources() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    fixture.put_storage("repair:target", compound("{items:[left,right]}"));
    assert_eq!(
        fixture
            .execute("data modify storage repair:target items insert 1 value literal")
            .unwrap(),
        1
    );
    let mut expected = vec!["left", "literal", "right"];
    for (index, provider) in SOURCES.iter().enumerate() {
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target items insert 1 from {provider} id"
                ))
                .unwrap(),
            1
        );
        expected.insert(1, before[index].get_string("id").unwrap());
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target items insert -1 string {provider} id 10"
                ))
                .unwrap(),
            1
        );
        expected.push(&before[index].get_string("id").unwrap()[10..]);
        let expected_tags: Vec<_> = expected
            .iter()
            .map(|value| NbtTag::String((*value).into()))
            .collect();
        assert_eq!(
            fixture.storage("repair:target").get_list("items"),
            Some(expected_tags.as_slice())
        );
    }

    let target = fixture.storage("repair:target");
    for suffix in [
        "insert value ignored",
        "insert from storage repair:source id",
        "insert string storage repair:source id",
        "insert 1",
        "insert value ignored 1",
    ] {
        assert!(
            fixture
                .execute(&format!("data modify storage repair:target items {suffix}"))
                .is_err(),
            "{suffix}"
        );
        assert_eq!(fixture.storage("repair:target"), target);
    }
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_string_keeps_provider_siblings_and_optional_bounds() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let mut expected = NbtCompound::new();
    for (index, provider) in SOURCES.iter().enumerate() {
        let id = before[index].get_string("id").unwrap();
        for (name, bounds, text) in [
            ("full", "", id),
            ("tail", " 10", &id[10..]),
            ("middle", " 10 -1", &id[10..id.len() - 1]),
        ] {
            let path = format!("{name}{index}");
            assert_eq!(
                fixture
                    .execute(&format!(
                        "data modify storage repair:target {path} set string {provider} id{bounds}"
                    ))
                    .unwrap(),
                1
            );
            expected.put_string(&path, text.to_owned());
            assert_eq!(fixture.storage("repair:target"), expected);
        }
    }
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target numeric set string storage repair:source number"
            )
            .unwrap(),
        1
    );
    expected.put_string("numeric", "42".to_owned());
    assert_eq!(
        fixture
            .execute("data modify storage repair:target negative set string storage repair:source text -3")
            .unwrap(),
        1
    );
    expected.put_string("negative", "def".to_owned());
    assert_eq!(fixture.storage("repair:target"), expected);
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_value_modes_and_entity_target_return_affected_counts() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    fixture.put_storage("repair:target", compound("{items:[2],rows:[{n:0},{n:0}]}"));
    for (suffix, expected_count) in [
        ("items append value 3", 1),
        ("items prepend value 1", 1),
        ("items insert -2 value 9", 1),
        ("items[0] set value 8", 1),
        // A missing compound is created before merging its contents.
        ("created merge value {key:7}", 1),
        ("rows[].n set value 5", 2),
    ] {
        assert_eq!(
            fixture
                .execute(&format!("data modify storage repair:target {suffix}"))
                .unwrap(),
            expected_count,
            "{suffix}"
        );
    }
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{items:[8,2,9,3],rows:[{n:5},{n:5}],created:{key:7}}")
    );
    assert_eq!(fixture.sources(), before);

    assert_eq!(
        fixture
            .execute("data modify entity @s Tags insert 0 value added")
            .unwrap(),
        1
    );
    let mut after = fixture.sources();
    // Entity scoreboard tags are a set, so their serialized order is unspecified.
    let mut tags: Vec<_> = after[1]
        .get_list("Tags")
        .unwrap()
        .iter()
        .map(|tag| tag.extract_string().unwrap())
        .collect();
    tags.sort_unstable();
    assert_eq!(tags, ["added", "original"]);
    after[1].put("Tags", before[1].get("Tags").unwrap().clone());
    assert_eq!(after[1], before[1]);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2], before[2]);
}

#[tokio::test]
async fn modify_unchanged_and_errors_preserve_nbt_through_dispatcher() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let target = compound("{id:'minecraft:storage',items:[1,2],scalar:7,object:{key:7}}");
    fixture.put_storage("repair:target", target.clone());

    // Each target provider reaches its executor, including block and entity
    // targets, and rejects a set of its current value without a write.
    for provider in SOURCES {
        let error = fixture
            .execute(&format!("data modify {provider} id set from {provider} id"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{provider}: {error:?}");
    }
    for provider in SOURCES {
        let error = fixture
            .execute(&format!(
                "data modify storage repair:target scalar set from {provider} missing"
            ))
            .unwrap_err();
        assert!(error.is(&ERROR_NOTHING_FOUND), "{provider}: {error:?}");
        // A whole source compound cannot be converted to a primitive string.
        let error = fixture
            .execute(&format!(
                "data modify storage repair:target scalar set string {provider}"
            ))
            .unwrap_err();
        assert!(error.is(&ERROR_EXPECTED_VALUE), "{provider}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), before);
    }

    let failures: [(&str, &'static dyn AnyCommandErrorType); 6] = [
        ("scalar set value 7", &ERROR_MERGE_UNCHANGED),
        ("object merge value {key:7}", &ERROR_MERGE_UNCHANGED),
        ("items insert 99 value 3", &ERROR_INVALID_INDEX),
        ("scalar append value 3", &ERROR_EXPECTED_LIST),
        ("object merge value 3", &ERROR_EXPECTED_OBJECT),
        (
            "scalar set string storage repair:source text 4 2",
            &ERROR_INVALID_SUBSTRING,
        ),
    ];
    for (suffix, expected_error) in failures {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(expected_error), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), before);
    }
}

#[tokio::test]
async fn modify_merge_recursively_retains_siblings_and_combines_sources_in_order() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound(
            "{parts:[{nested:{first:3,replace:4},list:[2],scalar:{inside:7},object:2},\
             {nested:{last:5,replace:6}}]}",
        ),
    );
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound(
            "{untouched:99,object:{sibling:2,nested:{keep:1,replace:0},\
             list:[1],scalar:0,object:{old:1}}}",
        ),
    );

    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target object merge from storage repair:source parts[]"
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound(
            "{untouched:99,object:{sibling:2,nested:{keep:1,first:3,last:5,replace:6},\
             list:[2],scalar:{inside:7},object:2}}"
        )
    );
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_counts_changed_compounds_and_rejects_unchanged_results() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound("{parts:[{nested:{value:1,extra:5}},{nested:{value:2}}]}"),
    );
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound(
            "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5}}},\
             {kind:hit,payload:{keep:2,nested:{value:0}}},\
             {kind:skip,payload:{keep:3,nested:{value:0}}}]}",
        ),
    );

    // Three compounds match, but the first already equals the final source
    // merge. Intermediate source changes do not increase the result count.
    assert_eq!(
        fixture
            .execute("data modify storage repair:target rows[].payload merge from storage repair:source parts[]")
            .unwrap(),
        2
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound(
            "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5}}},\
             {kind:hit,payload:{keep:2,nested:{value:2,extra:5}}},\
             {kind:skip,payload:{keep:3,nested:{value:2,extra:5}}}]}"
        )
    );
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target rows[{kind:hit}].payload merge value {added:8}"
            )
            .unwrap(),
        2
    );
    let expected = compound(
        "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5},added:8}},\
         {kind:hit,payload:{keep:2,nested:{value:2,extra:5},added:8}},\
         {kind:skip,payload:{keep:3,nested:{value:2,extra:5}}}]}",
    );
    assert_eq!(fixture.storage("repair:target"), expected);
    for suffix in [
        "rows[].payload merge from storage repair:source parts[]",
        "rows[{kind:hit}].payload merge value {added:8}",
        "rows[-1].payload merge value {}",
        "rows[0].payload{keep:99} merge value {added:8}",
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), expected);
    }
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_type_errors_validate_sources_before_changing_targets() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound("{mixed:[{value:{added:2}},{value:7}],number:42}"),
    );
    let sources = fixture.sources();
    let target = compound(
        "{object:{keep:1},scalar:7,numbers:[I;3,4],bytes:[B;3b,4b],longs:[L;3L,4L],empty:[I;]}",
    );
    fixture.put_storage("repair:target", target.clone());

    for (suffix, offending_tag) in [
        ("object merge from storage repair:source mixed[].value", "7"),
        (
            "missing.path merge from storage repair:source mixed[].value",
            "7",
        ),
        ("scalar merge value {added:2}", "7"),
        ("numbers[] merge value {added:2}", "3"),
        ("numbers[-1] merge value {added:2}", "4"),
        ("object merge string storage repair:source number", "\"42\""),
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert_eq!(
            error,
            ERROR_EXPECTED_OBJECT
                .create_without_context(TextComponent::text(offending_tag.to_owned())),
            "{suffix}"
        );
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), sources);
    }
    for path in [
        "numbers[].child",
        "numbers[-1].child",
        "bytes[].child",
        "bytes[-1].child",
        "longs[].child",
        "longs[-1].child",
    ] {
        let error = fixture
            .execute(&format!(
                "data modify storage repair:target {path} merge value {{added:2}}"
            ))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{path}: {error:?}");
    }
    let error = fixture
        .execute("data modify storage repair:target numbers[].child.deeper merge value {added:2}")
        .unwrap_err();
    assert_eq!(
        error,
        ERROR_NOTHING_FOUND.create_without_context(TextComponent::text("numbers[].child"))
    );
    for (provider, target_path, offending_tag) in [
        ("entity @s", "Tags[]", "\"original\""),
        ("block 0 64 0", "id", "\"minecraft:chest\""),
    ] {
        let error = fixture
            .execute(&format!(
                "data modify {provider} {target_path} merge value {{added:2}}"
            ))
            .unwrap_err();
        assert_eq!(
            error,
            ERROR_EXPECTED_OBJECT
                .create_without_context(TextComponent::text(offending_tag.to_owned())),
            "{provider}"
        );
    }
    assert_eq!(fixture.storage("repair:target"), target);
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_preserves_vanilla_storage_side_effects_on_failure() {
    let fixture = Fixture::new().await;
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound("{rows:[{value:{keep:1}},{value:7},{}],keep:9}"),
    );

    let error = fixture
        .execute("data modify storage repair:target rows[].value merge value {added:2}")
        .unwrap_err();
    assert_eq!(
        error,
        ERROR_EXPECTED_OBJECT.create_without_context(TextComponent::text("7"))
    );
    // Official storage returns live tags: all target paths are created first,
    // then the first compound is merged before the scalar aborts the command.
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{rows:[{value:{keep:1,added:2}},{value:7},{value:{}}],keep:9}")
    );

    fixture.put_storage("repair:target", compound("{keep:9,rows:[]}"));
    for (suffix, expected) in [
        (
            "created.nested merge value {}",
            "{keep:9,rows:[],created:{nested:{}}}",
        ),
        (
            "rows[{kind:new}] merge value {kind:new}",
            "{keep:9,rows:[{kind:new}],created:{nested:{}}}",
        ),
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), compound(expected));
    }
    assert_eq!(
        fixture
            .execute("data modify storage repair:target rows[{kind:other}] merge value {added:2}")
            .unwrap(),
        1
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{keep:9,rows:[{kind:new},{kind:other,added:2}],created:{nested:{}}}")
    );

    // Empty roots supplied for absent storage are not attached until success.
    let error = fixture
        .execute("data modify storage repair:absent created.nested merge value {}")
        .unwrap_err();
    assert!(error.is(&ERROR_MERGE_UNCHANGED));
    assert_eq!(fixture.storage("repair:absent"), NbtCompound::new());
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn removing_last_storage_tag_removes_the_live_key() {
    let fixture = Fixture::new().await;
    fixture
        .execute("data modify storage repair:removed value set value 9")
        .unwrap();
    assert_eq!(
        fixture
            .execute("data remove storage repair:removed value")
            .unwrap(),
        1
    );
    assert!(fixture.storage("repair:removed").is_empty());
    assert!(
        !fixture
            .source
            .server()
            .command_storage
            .lock()
            .unwrap()
            .contains_key("repair:removed")
    );
}

#[tokio::test]
async fn execute_data_conditions_treat_absent_paths_as_zero_matches_for_every_provider() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    for provider in SOURCES {
        for (condition, path, expected) in [
            ("if", "id", 42),
            ("unless", "id", 0),
            ("if", "missing.nested", 0),
            ("unless", "missing.nested", 42),
        ] {
            let command = format!(
                "execute {condition} data {provider} {path} run data get storage repair:source number"
            );
            assert_eq!(fixture.execute(&command).unwrap(), expected, "{command}");
        }
    }
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn storage_namespace_rejection_precedes_any_mutation() {
    let fixture = Fixture::new().await;
    let before = fixture
        .source
        .server()
        .command_storage
        .lock()
        .unwrap()
        .clone();
    for input in [
        "data get storage ..:unsafe",
        "data modify storage ..:unsafe value set value 1",
    ] {
        let error = fixture.execute(input).unwrap_err();
        assert!(error.context.is_some(), "{input}: {error:?}");
    }
    let accessor = StorageDataAccessor::new("..:unsafe".into(), fixture.source.server().clone());
    assert!(
        accessor
            .get_data()
            .unwrap_err()
            .is(&ERROR_STORAGE_NAMESPACE)
    );
    assert!(
        accessor
            .set_data(&compound("{value:1}"))
            .unwrap_err()
            .is(&ERROR_STORAGE_NAMESPACE)
    );
    assert_eq!(
        *fixture.source.server().command_storage.lock().unwrap(),
        before
    );
}

#[tokio::test]
async fn ordinary_data_merge_recursively_preserves_nested_siblings_and_detects_no_change() {
    let fixture = Fixture::new().await;
    fixture.put_storage("repair:nested", compound("{nested:{a:1,b:2}}"));
    assert_eq!(
        fixture
            .execute("data merge storage repair:nested {nested:{a:3}}")
            .unwrap(),
        1
    );
    assert_eq!(
        fixture.storage("repair:nested"),
        compound("{nested:{a:3,b:2}}")
    );
    let error = fixture
        .execute("data merge storage repair:nested {nested:{a:3}}")
        .unwrap_err();
    assert!(error.is(&ERROR_MERGE_UNCHANGED));

    let sign = Arc::new(SignBlockEntity::empty(BLOCK_POS));
    fixture.install(&Block::OAK_SIGN, sign);
    fixture
        .execute("data merge block 0 64 0 {front_text:{messages:['A','B','C','D'],color:red}} ")
        .unwrap();
    assert_eq!(
        fixture
            .execute("data merge block 0 64 0 {front_text:{has_glowing_text:1b}}")
            .unwrap(),
        1
    );
    let side = fixture
        .block_data()
        .get_compound("front_text")
        .unwrap()
        .clone();
    assert_eq!(side.get_string("color"), Some("red"));
    assert_eq!(
        side.get_list("messages").unwrap(),
        compound("{messages:['A','B','C','D']}")
            .get_list("messages")
            .unwrap()
    );
    assert!(
        fixture
            .execute("data merge block 0 64 0 {front_text:{has_glowing_text:1b}}")
            .unwrap_err()
            .is(&ERROR_MERGE_UNCHANGED)
    );
}

#[tokio::test]
async fn data_modify_reports_source_path_and_substring_errors_before_invalid_target() {
    let fixture = Fixture::new().await;
    let missing = fixture
        .execute("data modify block 9 64 9 Items set from storage repair:source absent")
        .unwrap_err();
    assert!(missing.is(&ERROR_NOTHING_FOUND));
    let substring = fixture
        .execute("data modify block 9 64 9 Items set string storage repair:source text 9 1")
        .unwrap_err();
    assert!(substring.is(&ERROR_INVALID_SUBSTRING));
    let target = fixture
        .execute("data modify block 9 64 9 Items set from storage repair:source number")
        .unwrap_err();
    assert!(target.is(&ERROR_BLOCK_INVALID));
}

#[tokio::test]
async fn pending_barrel_uses_live_datapack_source_on_first_access_and_after_data_reload() {
    use crate::data::datapack::loot_table_loader::parse_loot_table;
    use crate::world::loot::LootTableHandle;

    let fixture = Fixture::new().await;
    let server = fixture.source.server();
    let key = "repair:barrel_live";
    let table = parse_loot_table(r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:emerald"}]}]}"#).unwrap();
    server
        .datapack_manager
        .insert_loot_table(key.into(), Arc::new(table));
    assert!(matches!(
        server.datapack_manager.get_loot_table(key),
        Some(LootTableHandle::Dynamic(_))
    ));
    // A nonexistent static key proves the weak server upgrade matters.
    assert!(crate::world::loot::get_loot_table(key).is_none());
    fixture.set_state(BLOCK_POS, Block::BARREL.default_state.id);
    fixture
        .source
        .world()
        .block_entities
        .remove(&BLOCK_POS.chunk_position());
    fixture.source.world().add_block_entity_nbt(
        BLOCK_POS,
        &compound(
            "{id:'minecraft:barrel',x:0,y:64,z:0,LootTable:'repair:barrel_live',LootTableSeed:73L}",
        ),
    );
    let live = fixture.source.world().get_block_entity(&BLOCK_POS).unwrap();
    assert!(live.has_loot_table());
    let inventory = live.clone().get_inventory().unwrap();
    // This actual inventory read is the first unpack, before open/tick/data.
    let items: Vec<_> = (0..inventory.size())
        .map(|slot| inventory.get_stack(slot))
        .filter(|stack| !stack.is_empty())
        .collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item.id, Item::EMERALD.id);
    assert_eq!(items[0].item_count, 1);
    assert!(!live.has_loot_table());
    assert_eq!(
        fixture
            .execute("data merge block 0 64 0 {LootTable:'repair:barrel_live',LootTableSeed:74L}")
            .unwrap(),
        1
    );
    assert!(fixture.block_data().get("Items").is_none());
    assert_eq!(fixture.pending_data().get_string("LootTable"), Some(key));
    assert_eq!(fixture.pending_data().get_long("LootTableSeed"), Some(74));
    let reloaded = fixture.reload_saved_chunk();
    assert!(reloaded.has_loot_table());
    // Replace the live datapack table after reload; deferred access must look it
    // up then, rather than retaining the earlier generated result/table.
    let replacement = parse_loot_table(r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#).unwrap();
    server
        .datapack_manager
        .insert_loot_table(key.into(), Arc::new(replacement));
    let reloaded_inventory = reloaded.clone().get_inventory().unwrap();
    let items: Vec<_> = (0..reloaded_inventory.size())
        .map(|slot| reloaded_inventory.get_stack(slot))
        .filter(|stack| !stack.is_empty())
        .collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item.id, Item::DIAMOND.id);
    fixture.source.world().update_block_entity(&reloaded);
    assert!(fixture.pending_data().get("LootTable").is_none());
    let roundtrip = fixture.reload_saved_chunk().get_inventory().unwrap();
    assert_eq!(
        (0..roundtrip.size())
            .map(|slot| u32::from(roundtrip.get_stack(slot).item_count))
            .sum::<u32>(),
        1
    );
}

#[tokio::test]
async fn invalid_and_unsupported_block_targets_refuse_before_invoking_editor() {
    let fixture = Fixture::new().await;
    fixture.install(
        &Block::HOPPER,
        Arc::new(HopperBlockEntity::from_nbt(&NbtCompound::new(), BLOCK_POS)),
    );
    let unsupported = BlockDataAccessor::new(BLOCK_POS, fixture.source.world().clone()).unwrap();
    let pending = fixture.pending_data();
    let mut invoked = false;
    let error = unsupported
        .edit_data(&mut |_| {
            invoked = true;
            NbtEditDecision::Commit
        })
        .unwrap_err();
    assert!(error.is(&ERROR_BLOCK_DATA_UNSUPPORTED));
    assert!(!invoked);
    assert_eq!(fixture.pending_data(), pending);

    fixture.install(&Block::BARREL, Arc::new(BarrelBlockEntity::new(BLOCK_POS)));
    let stale = BlockDataAccessor::new(BLOCK_POS, fixture.source.world().clone()).unwrap();
    fixture.install(&Block::BARREL, Arc::new(BarrelBlockEntity::new(BLOCK_POS)));
    let pending = fixture.pending_data();
    let error = stale
        .edit_data(&mut |_| {
            invoked = true;
            NbtEditDecision::Commit
        })
        .unwrap_err();
    assert!(error.is(&ERROR_BLOCK_INVALID));
    assert!(!invoked);
    assert_eq!(fixture.pending_data(), pending);
}
