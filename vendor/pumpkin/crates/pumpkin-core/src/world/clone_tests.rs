//! ALL UNRUN: source-only World/BE foundation controls. These use registered
//! Worlds and real terrain/publication/inventory APIs, not the legacy clone route.
//! Future command integration must preflight every selected source and encoding
//! before any write, and supply its own reviewed inventory/menu admission boundary.

use super::*;
use crate::{
    block::{
        BlockBehaviour, BlockMetadata, OnStateReplacedArgs, PlacedArgs,
        entities::{barrel::BarrelBlockEntity, chest::ChestBlockEntity},
    },
    data::player_server::test::{PlayerFixture, close_fixture, player_fixture, stop_chunk_workers},
};
use pumpkin_data::{BlockId, block_properties::BarrelLikeProperties, item::Item};
use pumpkin_data::{
    data_component::DataComponent,
    data_component_impl::{DataComponentImpl, TooltipDisplayImpl},
};
use pumpkin_inventory::{
    Inventory, generic_container_screen_handler::create_generic_9x3, screen_handler::ScreenHandler,
};
use pumpkin_nbt::tag::NbtTag;
use pumpkin_world::chunk::io::Dirtiable;
use std::sync::{Mutex, atomic::AtomicUsize};

const SOURCE: BlockPos = BlockPos::new(2, 80, 2);
const TARGET: BlockPos = BlockPos::new(7, 80, 2);
const CONTAINERS: [(&str, &Block); 3] = [
    ("minecraft:chest", &Block::CHEST),
    ("minecraft:trapped_chest", &Block::TRAPPED_CHEST),
    ("minecraft:barrel", &Block::BARREL),
];

fn strict_flags() -> BlockFlags {
    BlockFlags::NOTIFY_LISTENERS
        | BlockFlags::UPDATE_KNOWN_SHAPE
        | BlockFlags::SKIP_DROPS
        | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
        | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
}

async fn fixture(registry: Option<Arc<BlockRegistry>>) -> (PlayerFixture, Arc<World>) {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let original = fixture.player.world();
    let world = if let Some(registry) = registry {
        let world = Arc::new(World::load(
            original.level.clone(),
            original.level_info.clone(),
            original.dimension.clone(),
            registry,
            Arc::downgrade(&fixture.server),
            Arc::default(),
        ));
        world.bind_self();
        fixture.server.worlds.rcu(|worlds| {
            let mut worlds = (**worlds).clone();
            worlds.push(world.clone());
            Arc::new(worlds)
        });
        world
    } else {
        original
    };
    // Existing stopped-worker fixture terrain; no generation/readiness claim.
    assert!(
        fixture
            .server
            .worlds
            .load()
            .iter()
            .any(|registered| Arc::ptr_eq(registered, &world))
    );
    set_state(&world, SOURCE, Block::AIR.default_state.id);
    set_state(&world, TARGET, Block::AIR.default_state.id);
    (fixture, world)
}

fn set_state(world: &World, position: BlockPos, state: BlockStateId) {
    let relative = position.chunk_relative_position();
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(relative.x as usize, relative.y, relative.z as usize, state);
        })
        .unwrap();
}

fn pending(world: &World, position: BlockPos) -> Option<NbtCompound> {
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .get(&position)
                .cloned()
        })
        .unwrap()
}

fn put_pending(world: &World, position: BlockPos, nbt: NbtCompound) {
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk
                .pending_block_entities
                .lock()
                .unwrap()
                .insert(position, nbt);
        })
        .unwrap();
}

fn container_nbt(kind: &str, position: BlockPos, count: u8) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    nbt.put_string("id", kind.to_owned());
    nbt.put_int("x", position.0.x);
    nbt.put_int("y", position.0.y);
    nbt.put_int("z", position.0.z);
    nbt.put_string("CustomName", "Vault".into());
    let mut item = NbtCompound::new();
    ItemStack::new(count, &Item::DIAMOND).write_item_stack(&mut item);
    item.put_byte("Slot", 0);
    nbt.put_list("Items", vec![NbtTag::Compound(item)]);
    nbt
}

fn saved(entity: &dyn BlockEntity) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    entity.write_internal(&mut nbt);
    nbt
}

fn assert_guards_released(world: &World, position: &BlockPos) {
    assert!(world.entity_lifecycle.try_lock().is_ok());
    assert!(
        !world
            .block_entities
            .try_get(&position.chunk_position())
            .is_locked()
    );
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            assert!(chunk.pending_block_entities.try_lock().is_ok());
        })
        .unwrap();
    // A callback can reacquire the unload owner too.
    drop(world.level.chunk_unload.access());
}

type Callback = Arc<dyn Fn(&Arc<World>, &BlockPos) + Send + Sync>;

#[derive(Default)]
struct Callbacks {
    removal: Option<Callback>,
    placement: Option<Callback>,
    removals: Arc<AtomicUsize>,
    placements: Arc<AtomicUsize>,
    deferred_removals: Arc<Mutex<Vec<BlockStateId>>>,
}

impl BlockMetadata for Callbacks {
    fn ids() -> Box<[BlockId]> {
        Box::new([BlockId::CHEST, BlockId::STONE, BlockId::BARREL])
    }
}

impl BlockBehaviour for Callbacks {
    fn affect_neighbors_after_removal(&self, args: OnStateReplacedArgs<'_>) {
        assert_guards_released(args.world, args.position);
        self.deferred_removals
            .lock()
            .unwrap()
            .push(args.old_state_id);
    }
    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        assert_guards_released(args.world, args.position);
        self.removals.fetch_add(1, Ordering::Relaxed);
        if let Some(callback) = &self.removal {
            callback(args.world, args.position);
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        assert_guards_released(args.world, args.position);
        self.placements.fetch_add(1, Ordering::Relaxed);
        if let Some(callback) = &self.placement {
            callback(args.world, args.position);
        }
    }
}

fn registry(callbacks: Callbacks) -> Arc<BlockRegistry> {
    let mut registry = BlockRegistry::default();
    registry.register(callbacks);
    Arc::new(registry)
}

struct RetiringEntity {
    position: BlockPos,
    callback: Callback,
    removed: Arc<AtomicUsize>,
    replaced: Arc<AtomicUsize>,
}

impl BlockEntity for RetiringEntity {
    fn resource_location(&self) -> &'static str {
        "minecraft:chest"
    }
    fn get_position(&self) -> BlockPos {
        self.position
    }
    fn write_nbt(&self, _nbt: &mut NbtCompound) {}
    fn from_nbt(_: &NbtCompound, _: BlockPos) -> Self {
        unreachable!()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_block_replaced(self: Arc<Self>, world: &Arc<World>, position: &BlockPos) {
        assert_guards_released(world, position);
        self.replaced.fetch_add(1, Ordering::Relaxed);
        (self.callback)(world, position);
    }
    fn on_removed(&self, world: &World) {
        assert_guards_released(world, &self.position);
        self.removed.fetch_add(1, Ordering::Relaxed);
    }
}

fn install_retiring(world: &World, callback: Callback) -> Arc<RetiringEntity> {
    world.remove_block_entity(&SOURCE);
    set_state(world, SOURCE, Block::CHEST.default_state.id);
    let entity = Arc::new(RetiringEntity {
        position: SOURCE,
        callback,
        removed: Arc::new(AtomicUsize::new(0)),
        replaced: Arc::new(AtomicUsize::new(0)),
    });
    world.add_block_entity(entity.clone());
    entity
}

fn open_barrel() -> BlockStateId {
    let mut properties = BarrelLikeProperties::from_state_id(Block::BARREL.default_state.id);
    properties.open = !properties.open;
    properties.to_state_id(&Block::BARREL)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_first_b3_stop_maps_current_block_without_running_stale_placement() {
    let placements = Arc::new(AtomicUsize::new(0));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        placements: placements.clone(),
        ..Callbacks::default()
    })))
    .await;
    for (replacement, expected) in [(Block::DIRT.default_state.id, false), (open_barrel(), true)] {
        install_retiring(
            &world,
            Arc::new(move |world, position| set_state(world, *position, replacement)),
        );
        assert_eq!(
            world.set_block_state_counted(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_LISTENERS
            ),
            expected
        );
        assert_eq!(
            world.get_block_state_id_if_loaded(&SOURCE),
            Some(replacement)
        );
        assert_eq!(placements.load(Ordering::Relaxed), 0);
        assert!(world.published_block_entity(&SOURCE).is_none());
        assert!(pending(&world, SOURCE).is_none());

        install_retiring(
            &world,
            Arc::new(move |world, position| set_state(world, *position, replacement)),
        );
        assert_eq!(
            world.set_block_state(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_LISTENERS
            ),
            replacement
        );
        install_retiring(
            &world,
            Arc::new(move |world, position| set_state(world, *position, replacement)),
        );
        assert_eq!(
            world.set_block_state_if(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_LISTENERS,
                |_| true
            ),
            Some(replacement)
        );
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_first_b3_stop_keeps_callback_owned_exact_target_entity_and_pending() {
    let placements = Arc::new(AtomicUsize::new(0));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        placements: placements.clone(),
        ..Callbacks::default()
    })))
    .await;
    let replacement = Arc::new(BarrelBlockEntity::new(SOURCE));
    replacement.set_stack(0, ItemStack::new(7, &Item::DIAMOND));
    let next = replacement.clone();
    let retired = install_retiring(
        &world,
        Arc::new(move |world, _| world.add_block_entity(next.clone())),
    );
    assert!(world.set_block_state_counted(
        &SOURCE,
        Block::BARREL.default_state.id,
        BlockFlags::NOTIFY_LISTENERS
    ));
    let replacement: Arc<dyn BlockEntity> = replacement;
    assert!(Arc::ptr_eq(
        &world.published_block_entity(&SOURCE).unwrap(),
        &replacement
    ));
    assert_eq!(pending(&world, SOURCE), Some(saved(replacement.as_ref())));
    assert_eq!(placements.load(Ordering::Relaxed), 0);
    assert_eq!(retired.removed.load(Ordering::Relaxed), 1);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_second_b3_stop_maps_different_and_same_block_states() {
    for (replacement, expected) in [(Block::DIRT.default_state.id, false), (open_barrel(), true)] {
        let placements = Arc::new(AtomicUsize::new(0));
        let (fixture, world) = fixture(Some(registry(Callbacks {
            removal: Some(Arc::new(move |world, position| {
                set_state(world, *position, replacement)
            })),
            placements: placements.clone(),
            ..Callbacks::default()
        })))
        .await;
        set_state(&world, SOURCE, Block::STONE.default_state.id);
        assert_eq!(
            world.set_block_state_counted(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_ALL
            ),
            expected
        );
        assert_eq!(
            world.get_block_state_id_if_loaded(&SOURCE),
            Some(replacement)
        );
        assert_eq!(placements.load(Ordering::Relaxed), 0);
        assert!(world.published_block_entity(&SOURCE).is_none());
        set_state(&world, SOURCE, Block::STONE.default_state.id);
        assert_eq!(
            world.set_block_state(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_ALL
            ),
            replacement
        );
        set_state(&world, SOURCE, Block::STONE.default_state.id);
        assert_eq!(
            world.set_block_state_if(
                &SOURCE,
                Block::BARREL.default_state.id,
                BlockFlags::NOTIFY_ALL,
                |_| true
            ),
            Some(replacement)
        );
        close_fixture(&fixture).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_second_b3_stop_keeps_callback_owned_exact_target_entity() {
    let replacement = Arc::new(BarrelBlockEntity::new(SOURCE));
    replacement.set_stack(0, ItemStack::new(9, &Item::DIAMOND));
    let next = replacement.clone();
    let placements = Arc::new(AtomicUsize::new(0));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        removal: Some(Arc::new(move |world, _| {
            world.add_block_entity(next.clone())
        })),
        placements: placements.clone(),
        ..Callbacks::default()
    })))
    .await;
    let retired = install_retiring(&world, Arc::new(|_, _| {}));
    assert!(world.set_block_state_counted(
        &SOURCE,
        Block::BARREL.default_state.id,
        BlockFlags::NOTIFY_ALL
    ));
    let replacement: Arc<dyn BlockEntity> = replacement;
    assert!(Arc::ptr_eq(
        &world.published_block_entity(&SOURCE).unwrap(),
        &replacement
    ));
    assert_eq!(pending(&world, SOURCE), Some(saved(replacement.as_ref())));
    assert_eq!(placements.load(Ordering::Relaxed), 0);
    assert_eq!(retired.removed.load(Ordering::Relaxed), 1);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_later_on_placed_replacement_does_not_revoke_changed_success() {
    let placements = Arc::new(AtomicUsize::new(0));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        placement: Some(Arc::new(|world, position| {
            world.set_block_state(position, Block::DIRT.default_state.id, strict_flags());
        })),
        placements: placements.clone(),
        ..Callbacks::default()
    })))
    .await;
    set_state(&world, SOURCE, Block::STONE.default_state.id);
    assert!(world.set_block_state_counted(
        &SOURCE,
        Block::BARREL.default_state.id,
        BlockFlags::NOTIFY_LISTENERS
    ));
    assert_eq!(world.get_block(&SOURCE), &Block::DIRT);
    assert_eq!(placements.load(Ordering::Relaxed), 1);
    assert!(world.published_block_entity(&SOURCE).is_none());
    set_state(&world, SOURCE, Block::STONE.default_state.id);
    assert_eq!(
        world.set_block_state(
            &SOURCE,
            Block::BARREL.default_state.id,
            BlockFlags::NOTIFY_LISTENERS
        ),
        Block::STONE.default_state.id
    );
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counted_refusals_and_legacy_force_state_keep_their_distinct_policy() {
    let (fixture, world) = fixture(None).await;
    set_state(&world, SOURCE, Block::STONE.default_state.id);
    world.unsent_block_changes.lock().unwrap().clear();
    assert!(!world.set_block_state_counted(
        &SOURCE,
        Block::STONE.default_state.id,
        BlockFlags::FORCE_STATE | BlockFlags::NOTIFY_LISTENERS
    ));
    assert!(world.unsent_block_changes.lock().unwrap().is_empty());
    assert_eq!(
        world.set_block_state(
            &SOURCE,
            Block::STONE.default_state.id,
            BlockFlags::NOTIFY_LISTENERS
        ),
        Block::STONE.default_state.id
    );
    assert!(world.unsent_block_changes.lock().unwrap().is_empty());
    assert_eq!(
        world.set_block_state_if(
            &SOURCE,
            Block::STONE.default_state.id,
            BlockFlags::FORCE_STATE | BlockFlags::NOTIFY_LISTENERS,
            |_| true
        ),
        Some(Block::STONE.default_state.id)
    );
    assert_eq!(
        world.unsent_block_changes.lock().unwrap().get(&SOURCE),
        Some(&Block::STONE.default_state.id)
    );
    for position in [
        BlockPos::new(512, 80, 0),
        BlockPos::new(0, world.get_top_y() + 1, 0),
        BlockPos::new(30_000_000, 80, 0),
    ] {
        assert!(!world.set_block_state_counted(
            &position,
            Block::BARREL.default_state.id,
            strict_flags()
        ));
        assert_eq!(
            world.set_block_state_if(
                &position,
                Block::BARREL.default_state.id,
                strict_flags(),
                |_| true
            ),
            None
        );
        assert!(world.published_block_entity(&position).is_none());
    }
    assert!(!world.is_debug());
    level_info::update_level_info(&world.level_info, |info| {
        info.world_gen_settings
            .dimensions
            .get_mut(world.dimension.minecraft_name)
            .unwrap()
            .generator
            .generator_type = "minecraft:debug".into();
    });
    assert!(world.is_debug());
    assert!(!world.set_block_state_counted(&SOURCE, Block::DIRT.default_state.id, strict_flags()));
    assert_eq!(world.get_block(&SOURCE), &Block::STONE);
    assert_eq!(
        world.set_block_state(&SOURCE, Block::DIRT.default_state.id, strict_flags()),
        Block::STONE.default_state.id
    );
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_attachment_and_terminal_cleanup_survive_callback_suppression() {
    let placements = Arc::new(AtomicUsize::new(0));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        placements: placements.clone(),
        ..Callbacks::default()
    })))
    .await;
    let old = install_retiring(
        &world,
        Arc::new(|_, _| panic!("suppressed removal callback")),
    );
    assert!(world.set_block_state_counted(&SOURCE, Block::BARREL.default_state.id, strict_flags()));
    let attached = world.published_block_entity(&SOURCE).unwrap();
    assert!(attached.as_any().is::<BarrelBlockEntity>());
    assert_eq!(pending(&world, SOURCE), Some(saved(attached.as_ref())));
    assert_eq!(old.replaced.load(Ordering::Relaxed), 0);
    assert_eq!(old.removed.load(Ordering::Relaxed), 1);
    assert_eq!(placements.load(Ordering::Relaxed), 0);
    assert!(world.entities.load().is_empty());
    // Ordinary dispatch is a positive control for the skipped added callback.
    world.set_block_state(&SOURCE, Block::STONE.default_state.id, strict_flags());
    assert!(world.set_block_state_counted(
        &SOURCE,
        Block::BARREL.default_state.id,
        BlockFlags::NOTIFY_LISTENERS
    ));
    assert_eq!(placements.load(Ordering::Relaxed), 1);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deferred_neighbor_walk_uses_previous_destination_without_replacing_attachment() {
    let removals = Arc::new(Mutex::new(Vec::new()));
    let (fixture, world) = fixture(Some(registry(Callbacks {
        deferred_removals: removals.clone(),
        ..Callbacks::default()
    })))
    .await;
    set_state(&world, TARGET, Block::STONE.default_state.id);
    assert!(world.set_block_state_counted(&TARGET, Block::BARREL.default_state.id, strict_flags()));
    assert!(removals.lock().unwrap().is_empty());
    let current = world.published_block_entity(&TARGET).unwrap();
    let canonical = pending(&world, TARGET);
    world.update_neighbours_on_block_set(&TARGET, Block::STONE.default_state.id);
    assert_eq!(
        *removals.lock().unwrap(),
        vec![Block::STONE.default_state.id]
    );
    assert!(Arc::ptr_eq(
        &world.published_block_entity(&TARGET).unwrap(),
        &current
    ));
    assert_eq!(pending(&world, TARGET), canonical);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_capture_uses_one_current_state_without_pending_overlay_or_live_mark() {
    let (fixture, world) = fixture(None).await;
    for (kind, block) in CONTAINERS {
        let mut equivalent = None;
        for explicit_empty in [false, true] {
            world.remove_block_entity(&SOURCE);
            set_state(&world, SOURCE, block.default_state.id);
            let mut nbt = container_nbt(kind, SOURCE, 3);
            if explicit_empty {
                nbt.put_compound("components", NbtCompound::new());
            }
            let live = block_entity_from_nbt(&nbt).unwrap();
            world.add_block_entity(live.clone());
            // Change live slots without publishing. A pending overlay would mix
            // this version with the deliberately incompatible raw metadata below.
            live.clone()
                .get_inventory()
                .unwrap()
                .set_stack(0, ItemStack::new(7, &Item::DIAMOND));
            let mut stale = nbt.clone();
            stale.put_string("CustomName", "Stale".into());
            let mut opaque = NbtCompound::new();
            opaque.put_int("unknown", 1);
            stale.put_compound("components", opaque);
            put_pending(&world, SOURCE, stale.clone());
            let chunk = world
                .level
                .loaded_chunks
                .get(&SOURCE.chunk_position())
                .unwrap()
                .value()
                .clone();
            chunk.mark_dirty(false);
            world
                .level
                .chunk_unload
                .access()
                .mark_prepared(SOURCE.chunk_position(), &chunk);
            let payload = world
                .snapshot_block_entity_for_clone(&SOURCE)
                .unwrap()
                .unwrap();
            assert_eq!(payload.kind, kind);
            assert_eq!(payload.custom.get_string("CustomName"), Some("Vault"));
            assert_eq!(
                payload.custom.get_list("Items").unwrap()[0]
                    .extract_compound()
                    .unwrap()
                    .get_int("count"),
                Some(7)
            );
            for field in ["id", "x", "y", "z", "components"] {
                assert!(payload.custom.get(field).is_none());
            }
            let full = payload.full_metadata(SOURCE);
            assert_eq!(full.get_string("id"), Some(kind));
            assert_eq!(full.get_int("x"), Some(SOURCE.0.x));
            assert_eq!(full.get_int("y"), Some(SOURCE.0.y));
            assert_eq!(full.get_int("z"), Some(SOURCE.0.z));
            assert!(full.get_compound("components").unwrap().is_empty());
            if let Some(previous) = &equivalent {
                assert_eq!(previous, &payload);
            }
            equivalent = Some(payload);
            assert!(Arc::ptr_eq(
                &world.published_block_entity(&SOURCE).unwrap(),
                &live
            ));
            assert_eq!(pending(&world, SOURCE), Some(stale));
            assert!(!chunk.is_dirty());
            assert!(
                world
                    .level
                    .chunk_unload
                    .access()
                    .prepare_or_defer(SOURCE.chunk_position(), &chunk)
            );
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_capture_is_detached_and_normalizes_absent_or_empty_components() {
    let (fixture, world) = fixture(None).await;
    for (kind, block) in CONTAINERS {
        let mut equivalent = None;
        world.remove_block_entity(&SOURCE);
        set_state(&world, SOURCE, block.default_state.id);
        for explicit_empty in [false, true] {
            let mut nbt = container_nbt(kind, SOURCE, 4);
            if explicit_empty {
                nbt.put_compound("components", NbtCompound::new());
            }
            put_pending(&world, SOURCE, nbt.clone());
            let chunk = world
                .level
                .loaded_chunks
                .get(&SOURCE.chunk_position())
                .unwrap()
                .value()
                .clone();
            chunk.mark_dirty(false);
            world
                .level
                .chunk_unload
                .access()
                .mark_prepared(SOURCE.chunk_position(), &chunk);
            let map_count = world.block_entities.len();
            let payload = world
                .snapshot_block_entity_for_clone(&SOURCE)
                .unwrap()
                .unwrap();
            assert_eq!(payload.custom.get_string("CustomName"), Some("Vault"));
            assert_eq!(payload.custom.get_list("Items"), nbt.get_list("Items"));
            assert!(payload.components.is_empty());
            assert!(
                payload
                    .full_metadata(SOURCE)
                    .get_compound("components")
                    .unwrap()
                    .is_empty()
            );
            if let Some(previous) = &equivalent {
                assert_eq!(previous, &payload);
            }
            equivalent = Some(payload);
            assert!(world.published_block_entity(&SOURCE).is_none());
            assert_eq!(world.block_entities.len(), map_count);
            assert_eq!(pending(&world, SOURCE), Some(nbt));
            assert!(!chunk.is_dirty());
            assert!(
                world
                    .level
                    .chunk_unload
                    .access()
                    .prepare_or_defer(SOURCE.chunk_position(), &chunk)
            );
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_and_pending_deferred_loot_capture_does_not_open_inventory() {
    let (fixture, world) = fixture(None).await;
    for (kind, block) in CONTAINERS {
        let mut equivalent = None;
        for live_source in [false, true] {
            world.remove_block_entity(&SOURCE);
            set_state(&world, SOURCE, block.default_state.id);
            let mut nbt = container_nbt(kind, SOURCE, 4);
            nbt.put_string("LootTable", "ferraloom:missing".into());
            nbt.put_long("LootTableSeed", 37);
            // The existing raw admission contract, like the supported decoder,
            // bypasses Items entirely for a valid deferred identifier.
            nbt.put_string("Items", "ignored while deferred".into());
            let live = live_source.then(|| block_entity_from_nbt(&nbt).unwrap());
            if let Some(live) = &live {
                world.add_block_entity(live.clone());
            } else {
                put_pending(&world, SOURCE, nbt);
            }
            let before = pending(&world, SOURCE);
            let payload = world
                .snapshot_block_entity_for_clone(&SOURCE)
                .unwrap()
                .unwrap();
            assert_eq!(
                payload.custom.get_string("LootTable"),
                Some("ferraloom:missing")
            );
            assert_eq!(payload.custom.get_long("LootTableSeed"), Some(37));
            assert!(payload.custom.get("Items").is_none());
            if let Some(previous) = &equivalent {
                assert_eq!(previous, &payload);
            }
            equivalent = Some(payload.clone());
            assert_eq!(pending(&world, SOURCE), before);
            if let Some(live) = &live {
                assert!(live.has_loot_table());
            } else {
                assert!(world.published_block_entity(&SOURCE).is_none());
            }
            world.remove_block_entity(&TARGET);
            set_state(&world, TARGET, Block::AIR.default_state.id);
            assert!(world.set_block_state_counted(&TARGET, block.default_state.id, strict_flags()));
            let destination = world.published_block_entity(&TARGET).unwrap();
            assert!(
                world
                    .restore_cloned_block_entity(&TARGET, &payload)
                    .unwrap()
            );
            assert!(destination.has_loot_table());
            assert_eq!(
                pending(&world, TARGET).unwrap().get_int("x"),
                Some(TARGET.0.x)
            );
            // This is intentionally after capture/restore: a preserved target
            // server binding resolves the missing table to empty on first access.
            assert!(
                destination
                    .clone()
                    .get_inventory()
                    .unwrap()
                    .get_stack(0)
                    .is_empty()
            );
            assert!(!destination.has_loot_table());
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restore_uses_current_target_arc_retains_real_menu_and_inventory_and_target_identity() {
    let (fixture, world) = fixture(None).await;
    set_state(&world, SOURCE, Block::CHEST.default_state.id);
    put_pending(&world, SOURCE, container_nbt("minecraft:chest", SOURCE, 6));
    let payload = world
        .snapshot_block_entity_for_clone(&SOURCE)
        .unwrap()
        .unwrap();
    set_state(&world, TARGET, Block::CHEST.default_state.id);
    let old = Arc::new(ChestBlockEntity::new(TARGET));
    old.set_stack(0, ItemStack::new(1, &Item::APPLE));
    world.add_block_entity(old.clone());
    let old_inventory = old.clone().get_inventory().unwrap();
    let old_snapshot = saved(old.as_ref());
    let current = Arc::new(ChestBlockEntity::new(TARGET));
    current.set_stack(0, ItemStack::new(2, &Item::APPLE));
    world.add_block_entity(current.clone());
    let inventory = current.clone().get_inventory().unwrap();
    let menu_inventory = current.clone().get_menu_inventory(&world).unwrap();
    let mut menu = create_generic_9x3(
        1,
        fixture.player.inventory(),
        menu_inventory.clone(),
        fixture.player.as_ref(),
    );
    let slot = menu.get_behaviour().slots[0].clone();
    let token = current.load_token();
    assert!(
        world
            .restore_cloned_block_entity(&TARGET, &payload)
            .unwrap()
    );
    let current_entity: Arc<dyn BlockEntity> = current.clone();
    assert!(Arc::ptr_eq(
        &world.published_block_entity(&TARGET).unwrap(),
        &current_entity
    ));
    assert!(Arc::ptr_eq(
        &inventory,
        &current.clone().get_inventory().unwrap()
    ));
    assert!(Arc::ptr_eq(&slot, &menu.get_behaviour().slots[0]));
    assert!(!Arc::ptr_eq(&token, &current.load_token()));
    assert!(
        slot.get_stack()
            .are_equal(&ItemStack::new(6, &Item::DIAMOND))
    );
    assert!(
        menu_inventory
            .get_stack(0)
            .are_equal(&inventory.get_stack(0))
    );
    assert_eq!(current.get_viewer_count(), 1);
    let canonical = pending(&world, TARGET).unwrap();
    assert_eq!(canonical, saved(current.as_ref()));
    assert_eq!(canonical.get_string("id"), Some("minecraft:chest"));
    assert_eq!(canonical.get_int("x"), Some(TARGET.0.x));
    assert_eq!(canonical.get_int("y"), Some(TARGET.0.y));
    assert_eq!(canonical.get_int("z"), Some(TARGET.0.z));
    assert!(canonical.get_compound("components").unwrap().is_empty());
    assert_eq!(canonical.get_string("CustomName"), Some("Vault"));
    assert_eq!(saved(old.as_ref()), old_snapshot);
    assert_eq!(old_inventory.get_stack(0).item_count, 1);
    slot.set_stack(ItemStack::new(3, &Item::APPLE));
    assert_eq!(inventory.get_stack(0).item_count, 3);
    menu.on_closed(fixture.player.as_ref());
    assert_eq!(current.get_viewer_count(), 0);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_refusals_preserve_actual_terrain_records_and_never_publish() {
    let (fixture, world) = fixture(None).await;
    let valid = container_nbt("minecraft:chest", SOURCE, 3);
    let mut cases = Vec::new();
    let mut wrong_id = valid.clone();
    wrong_id.put_string("id", "minecraft:barrel".into());
    cases.push(wrong_id);
    let mut wrong_pos = valid.clone();
    wrong_pos.put_int("x", SOURCE.0.x + 1);
    cases.push(wrong_pos);
    let mut wrong_items = valid.clone();
    wrong_items.put_string("Items", "not a list".into());
    cases.push(wrong_items);
    let mut wrong_slot = valid.clone();
    let mut item = NbtCompound::new();
    ItemStack::new(1, &Item::DIAMOND).write_item_stack(&mut item);
    item.put_byte("Slot", 27);
    wrong_slot.put_list("Items", vec![NbtTag::Compound(item)]);
    cases.push(wrong_slot);
    let mut components = valid.clone();
    let mut opaque = NbtCompound::new();
    opaque.put_int("unknown", 1);
    components.put_compound("components", opaque.clone());
    cases.push(components);
    let mut rich_name = valid.clone();
    rich_name.put_compound("CustomName", opaque.clone());
    cases.push(rich_name);
    let mut locked = valid.clone();
    locked.put_compound("lock", NbtCompound::new());
    cases.push(locked);
    let mut unknown = valid.clone();
    unknown.put_compound("unknown", opaque);
    cases.push(unknown);
    let mut bad_loot = valid.clone();
    bad_loot.put_string("LootTable", "invalid key with spaces".into());
    cases.push(bad_loot);
    let mut bad_encoding = valid.clone();
    bad_encoding.put_string("CustomName", "x".repeat(65_536));
    cases.push(bad_encoding);
    set_state(&world, SOURCE, Block::CHEST.default_state.id);
    // A separate supported source remains untouched too. This is capture-only,
    // not a claim that the unwired command preflights every selected source.
    set_state(&world, TARGET, Block::CHEST.default_state.id);
    let first = Arc::new(ChestBlockEntity::new(TARGET));
    first.set_stack(0, ItemStack::new(2, &Item::APPLE));
    world.add_block_entity(first.clone());
    let first_pending = pending(&world, TARGET);
    for nbt in cases {
        put_pending(&world, SOURCE, nbt.clone());
        let maps = world.block_entities.len();
        let before = saved(first.as_ref());
        assert!(matches!(
            world.snapshot_block_entity_for_clone(&SOURCE),
            Err(BlockEntityLoadError::Unsupported(_))
        ));
        assert!(world.published_block_entity(&SOURCE).is_none());
        assert_eq!(world.block_entities.len(), maps);
        assert_eq!(world.get_block(&SOURCE), &Block::CHEST);
        assert_eq!(pending(&world, SOURCE), Some(nbt));
        assert_eq!(saved(first.as_ref()), before);
        assert_eq!(pending(&world, TARGET), first_pending);
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_metadata_and_writerless_item_refusals_leave_live_and_pending_unchanged() {
    let (fixture, world) = fixture(None).await;
    set_state(&world, SOURCE, Block::CHEST.default_state.id);
    for field in ["CustomName", "lock", "components"] {
        world.remove_block_entity(&SOURCE);
        let mut nbt = container_nbt("minecraft:chest", SOURCE, 3);
        let mut opaque = NbtCompound::new();
        opaque.put_int("unknown", 1);
        nbt.put_compound(field, opaque);
        let live = block_entity_from_nbt(&nbt).unwrap();
        world.add_block_entity(live.clone());
        let before = saved(live.as_ref());
        let raw = pending(&world, SOURCE);
        assert!(matches!(
            world.snapshot_block_entity_for_clone(&SOURCE),
            Err(BlockEntityLoadError::Unsupported(_))
        ));
        assert_eq!(saved(live.as_ref()), before);
        assert_eq!(pending(&world, SOURCE), raw);
        assert!(Arc::ptr_eq(
            &world.published_block_entity(&SOURCE).unwrap(),
            &live
        ));
    }
    world.remove_block_entity(&SOURCE);
    let live = Arc::new(ChestBlockEntity::new(SOURCE));
    world.add_block_entity(live.clone());
    let before_pending = pending(&world, SOURCE);
    live.set_stack(
        0,
        ItemStack::new_with_component(
            1,
            &Item::STICK,
            vec![(
                DataComponent::TooltipDisplay,
                Some(TooltipDisplayImpl.to_dyn()),
            )],
        ),
    );
    let token = live.load_token();
    assert!(matches!(
        world.snapshot_block_entity_for_clone(&SOURCE),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    assert_eq!(pending(&world, SOURCE), before_pending);
    assert!(Arc::ptr_eq(&token, &live.load_token()));
    assert_eq!(live.get_stack(0).patch.len(), 1);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_missing_and_mismatched_entities_are_refusals_only_true_non_be_is_none() {
    let (fixture, world) = fixture(None).await;
    set_state(&world, SOURCE, Block::STONE.default_state.id);
    assert_eq!(
        world.snapshot_block_entity_for_clone(&SOURCE).unwrap(),
        None
    );
    set_state(&world, SOURCE, Block::CHEST.default_state.id);
    assert!(matches!(
        world.snapshot_block_entity_for_clone(&SOURCE),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    let live = Arc::new(ChestBlockEntity::new(SOURCE));
    world.add_block_entity(live.clone());
    set_state(&world, SOURCE, Block::BARREL.default_state.id);
    let raw = pending(&world, SOURCE);
    assert!(matches!(
        world.snapshot_block_entity_for_clone(&SOURCE),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    assert_eq!(pending(&world, SOURCE), raw);
    world.remove_block_entity(&SOURCE);
    set_state(&world, SOURCE, Block::JUKEBOX.default_state.id);
    let jukebox = crate::block::entities::create_block_entity(
        Block::JUKEBOX.default_state.block_entity_type,
        SOURCE,
    )
    .unwrap();
    world.add_block_entity(jukebox.clone());
    let raw = pending(&world, SOURCE);
    assert!(matches!(
        world.snapshot_block_entity_for_clone(&SOURCE),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    assert!(Arc::ptr_eq(
        &world.published_block_entity(&SOURCE).unwrap(),
        &jukebox
    ));
    assert_eq!(pending(&world, SOURCE), raw);
    world.remove_block_entity(&SOURCE);
    put_pending(&world, SOURCE, raw.unwrap());
    let raw = pending(&world, SOURCE);
    assert!(matches!(
        world.snapshot_block_entity_for_clone(&SOURCE),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    assert!(world.published_block_entity(&SOURCE).is_none());
    assert_eq!(pending(&world, SOURCE), raw);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restore_refuses_invalid_payload_before_commit_and_missing_target_skips_terrain_write() {
    let (fixture, world) = fixture(None).await;
    set_state(&world, SOURCE, Block::CHEST.default_state.id);
    put_pending(&world, SOURCE, container_nbt("minecraft:chest", SOURCE, 3));
    let payload = world
        .snapshot_block_entity_for_clone(&SOURCE)
        .unwrap()
        .unwrap();
    set_state(&world, TARGET, Block::CHEST.default_state.id);
    let live = Arc::new(ChestBlockEntity::new(TARGET));
    live.set_stack(0, ItemStack::new(2, &Item::APPLE));
    world.add_block_entity(live.clone());
    let mut bad = payload.clone();
    bad.custom.put_string("Items", "invalid".into());
    let before = saved(live.as_ref());
    let raw = pending(&world, TARGET);
    let token = live.load_token();
    assert!(matches!(
        world.restore_cloned_block_entity(&TARGET, &bad),
        Err(BlockEntityLoadError::Unsupported(_))
    ));
    assert_eq!(saved(live.as_ref()), before);
    assert_eq!(pending(&world, TARGET), raw);
    assert!(Arc::ptr_eq(&token, &live.load_token()));
    world.remove_block_entity(&TARGET);
    let state = world.get_block_state_id_if_loaded(&TARGET);
    assert!(
        !world
            .restore_cloned_block_entity(&TARGET, &payload)
            .unwrap()
    );
    assert_eq!(world.get_block_state_id_if_loaded(&TARGET), state);
    assert!(pending(&world, TARGET).is_none());
    assert!(world.published_block_entity(&TARGET).is_none());
    close_fixture(&fixture).await;
}
