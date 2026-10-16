use super::*;
use crate::{
    block::entities::{block_entity_from_nbt, chest::ChestBlockEntity},
    world::World,
};
use arc_swap::ArcSwap;
use pumpkin_config::world::LevelConfig;
use pumpkin_data::{dimension::Dimension, item::Item, item_stack::ItemStack};
use pumpkin_inventory::Inventory;
use pumpkin_util::world_seed::Seed;
use pumpkin_world::{chunk::ChunkData, level::Level, world_info::LevelData};
use std::sync::{Arc, Weak};

/// A manually loaded Full chunk fixture tests command functionality, not the
/// scheduler or its readiness guarantees. No listeners or server ticks run here.
async fn world() -> (tempfile::TempDir, Arc<World>) {
    world_in_dimension(Dimension::OVERWORLD).await
}

async fn world_in_dimension(dimension: Dimension) -> (tempfile::TempDir, Arc<World>) {
    let folder = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        folder.path().into(),
        0,
        dimension.clone(),
    );
    level.shutdown().await.unwrap();
    for x in 0..=1 {
        level
            .loaded_chunks
            .insert(Vector2::new(x, 0), ChunkData::empty_sync(x, 0));
    }
    let world = Arc::new(World::load(
        level,
        Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
        dimension,
        crate::block::registry::default_registry(),
        Weak::new(),
        Arc::default(),
    ));
    (folder, world)
}

fn dispatcher() -> CommandDispatcher {
    let mut dispatcher = CommandDispatcher::new();
    let registry = PermissionRegistry::default();
    register(&mut dispatcher, &registry);
    crate::command::commands::setblock::register(&mut dispatcher, &registry);
    dispatcher
}

fn put(world: &World, pos: BlockPos, state: BlockStateId) {
    world
        .level
        .read_chunk_sync(&pos.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(
                (pos.0.x & 15) as usize,
                pos.0.y,
                (pos.0.z & 15) as usize,
                state,
            );
        })
        .unwrap();
}

fn dropped_count(world: &World, item: &'static Item) -> u32 {
    world
        .entities
        .load()
        .iter()
        .filter_map(|entity| entity.get_item_entity())
        .map(|entity| {
            let stack = entity.get_item_stack().lock().unwrap();
            if stack.item.id == item.id {
                u32::from(stack.item_count)
            } else {
                0
            }
        })
        .sum()
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep destroy-mode drops, entities and block-state checks under one dispatcher fixture."
)]
async fn destroy_commands() {
    let (_folder, world) = world().await;
    let dispatcher = dispatcher();
    let source = CommandSource::dummy().with_world(world.clone());
    for z in 0..=1 {
        for x in 0..=1 {
            put(
                &world,
                BlockPos::new(x, 100, z),
                Block::STONE.default_state.id,
            );
        }
    }
    let chest_pos = BlockPos::new(0, 100, 0);
    put(&world, chest_pos, Block::CHEST.default_state.id);
    let chest = Arc::new(ChestBlockEntity::new(chest_pos));
    chest.set_stack(0, ItemStack::new(8, &Item::DIAMOND));
    world.add_block_entity(chest.clone());
    assert!(!world.get_block_state(&chest_pos).is_air());

    assert_eq!(
        dispatcher
            .execute_input("fill 0 100 0 1 100 1 gold_block destroy", &source)
            .unwrap(),
        4
    );
    for z in 0..=1 {
        for x in 0..=1 {
            assert_eq!(
                world.get_block_state_id(&BlockPos::new(x, 100, z)),
                Block::GOLD_BLOCK.default_state.id
            );
        }
    }
    assert!(chest.is_empty());
    assert!(world.get_block_entity(&chest_pos).is_none());
    assert!(
        world
            .level
            .read_chunk_sync(&chest_pos.chunk_position(), |chunk| {
                !chunk
                    .pending_block_entities
                    .lock()
                    .unwrap()
                    .contains_key(&chest_pos)
            })
            .unwrap()
    );
    assert_eq!(dropped_count(&world, &Item::DIAMOND), 8);
    assert_eq!(dropped_count(&world, &Item::CHEST), 1);
    assert_eq!(dropped_count(&world, &Item::COBBLESTONE), 3);

    // Java destroys even an identical target, and counts destruction when the
    // requested air replacement itself makes no subsequent state change.
    assert_eq!(
        dispatcher
            .execute_input("fill 0 100 0 0 100 0 gold_block destroy", &source)
            .unwrap(),
        1
    );
    assert_eq!(dropped_count(&world, &Item::GOLD_BLOCK), 1);
    assert_eq!(
        dispatcher
            .execute_input(
                "fill 0 100 0 1 100 1 glass replace gold_block destroy",
                &source
            )
            .unwrap(),
        4
    );
    assert_eq!(
        dispatcher
            .execute_input("fill 0 100 0 1 100 1 air destroy", &source)
            .unwrap(),
        4
    );
    assert!(
        dispatcher
            .execute_input("fill 0 100 0 1 100 1 air destroy", &source)
            .is_err()
    );

    // These actual inventory-drop callbacks cross a chunk boundary and expose
    // traversal order: X fastest, then Y, then Z, rather than chunk-major order.
    let order = [
        BlockPos::new(15, 110, 4),
        BlockPos::new(16, 110, 4),
        BlockPos::new(15, 111, 4),
        BlockPos::new(16, 111, 4),
        BlockPos::new(15, 110, 5),
        BlockPos::new(16, 110, 5),
        BlockPos::new(15, 111, 5),
        BlockPos::new(16, 111, 5),
    ];
    for pos in order {
        put(&world, pos, Block::CHEST.default_state.id);
        let chest = Arc::new(ChestBlockEntity::new(pos));
        chest.set_stack(0, ItemStack::new(1, &Item::EMERALD));
        world.add_block_entity(chest);
    }
    assert_eq!(
        dispatcher
            .execute_input("fill 15 110 4 16 111 5 air destroy", &source)
            .unwrap(),
        8
    );
    let dropped_in_order: Vec<_> = world
        .entities
        .load()
        .iter()
        .filter_map(|entity| {
            let item = entity.get_item_entity()?;
            if item.get_item_stack().lock().unwrap().item.id != Item::EMERALD.id {
                return None;
            }
            let pos = entity.get_entity().pos.load();
            Some(BlockPos::new(
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ))
        })
        .collect();
    assert_eq!(dropped_in_order, order);
    assert_eq!(dropped_count(&world, &Item::EMERALD), 8);
    assert_eq!(dropped_count(&world, &Item::DIAMOND), 8);
}

async fn destroy_callbacks_shapes_and_fluids() {
    use pumpkin_data::block_properties::{Facing, OakFenceLikeProperties, ObserverLikeProperties};

    let (_folder, world) = world().await;
    let dispatcher = dispatcher();
    let source = CommandSource::dummy().with_world(world.clone());

    let sponge = BlockPos::new(4, 100, 4);
    let water = BlockPos::new(5, 100, 4);
    put(&world, sponge, Block::STONE.default_state.id);
    put(&world, water, Block::WATER.default_state.id);
    assert_eq!(
        dispatcher
            .execute_input("fill 4 100 4 4 100 4 sponge destroy", &source)
            .unwrap(),
        1
    );
    assert_eq!(world.get_block(&sponge), &Block::WET_SPONGE);
    assert!(world.get_block_state(&water).is_air());
    let client_changes: std::collections::HashMap<_, _> =
        world.take_flushed_block_changes().into_iter().collect();
    assert_eq!(client_changes[&sponge], Block::WET_SPONGE.default_state.id);
    assert_eq!(client_changes[&water], BlockStateId::AIR);

    let fence = BlockPos::new(8, 100, 4);
    let neighbor_fence = BlockPos::new(9, 100, 4);
    put(&world, fence, Block::STONE.default_state.id);
    put(&world, neighbor_fence, Block::OAK_FENCE.default_state.id);
    assert_eq!(
        dispatcher
            .execute_input("fill 8 100 4 8 100 4 oak_fence destroy", &source)
            .unwrap(),
        1
    );
    let placed = OakFenceLikeProperties::from_state_id(world.get_block_state_id(&fence));
    let neighbor = OakFenceLikeProperties::from_state_id(world.get_block_state_id(&neighbor_fence));
    assert!(placed.east, "the new fence must fold its neighbour shapes");
    assert!(
        neighbor.west,
        "placement must update the existing fence shape"
    );
    let client_changes: std::collections::HashMap<_, _> =
        world.take_flushed_block_changes().into_iter().collect();
    assert_eq!(client_changes[&fence], world.get_block_state_id(&fence));
    assert_eq!(
        client_changes[&neighbor_fence],
        world.get_block_state_id(&neighbor_fence)
    );

    // An explicit property wins after the initial shape fold. This isolated
    // fence has no neighbours whose subsequent shape change could alter it.
    let explicit_fence = BlockPos::new(12, 100, 4);
    put(&world, explicit_fence, Block::STONE.default_state.id);
    assert_eq!(
        dispatcher
            .execute_input(
                "fill 12 100 4 12 100 4 oak_fence[east=true] destroy",
                &source
            )
            .unwrap(),
        1
    );
    let placed = OakFenceLikeProperties::from_state_id(world.get_block_state_id(&explicit_fence));
    assert!(placed.east);
    world.take_flushed_block_changes();

    let source_water = BlockPos::new(6, 100, 8);
    let observer = BlockPos::new(5, 100, 8);
    let mut observer_props = ObserverLikeProperties::default(&Block::OBSERVER);
    observer_props.facing = Facing::East;
    put(
        &world,
        observer,
        observer_props.to_state_id(&Block::OBSERVER),
    );
    put(&world, source_water, Block::WATER.default_state.id);
    assert!(!world.is_block_tick_scheduled(&observer, &Block::OBSERVER));
    assert!(
        dispatcher
            .execute_input("fill 6 100 8 6 100 8 water destroy", &source)
            .is_err(),
        "preserving the same source fluid changes no blocks"
    );
    assert_eq!(
        world.get_block_state_id(&source_water),
        Block::WATER.default_state.id
    );
    assert!(!world.is_block_tick_scheduled(&observer, &Block::OBSERVER));
    // Failed fills do not flush, so explicitly inspect any mistakenly queued
    // transition as well as the observer's scheduled pulse.
    world.flush_block_updates();
    assert!(world.take_flushed_block_changes().is_empty());

    let (_nether_folder, nether) = world_in_dimension(Dimension::THE_NETHER).await;
    let nether_source = CommandSource::dummy().with_world(nether.clone());
    let sponge = BlockPos::new(4, 100, 4);
    put(&nether, sponge, Block::STONE.default_state.id);
    assert_eq!(
        dispatcher
            .execute_input("fill 4 100 4 4 100 4 wet_sponge destroy", &nether_source)
            .unwrap(),
        1
    );
    assert_eq!(nether.get_block(&sponge), &Block::SPONGE);
    let client_changes: std::collections::HashMap<_, _> =
        nether.take_flushed_block_changes().into_iter().collect();
    assert_eq!(client_changes[&sponge], Block::SPONGE.default_state.id);
}

async fn destroy_deferred_removal_preserves_lever_notifications() {
    use crate::block::{BlockBehaviour, BlockMetadata, OnNeighborUpdateArgs};
    use pumpkin_data::BlockId;
    use pumpkin_data::block_properties::{AttachFace, LeverLikeProperties};
    use std::sync::Mutex;

    struct ObserveNeighbors(Arc<Mutex<Vec<BlockStateId>>>);
    impl BlockMetadata for ObserveNeighbors {
        fn ids() -> Box<[BlockId]> {
            [BlockId::STONE].into()
        }
    }
    impl BlockBehaviour for ObserveNeighbors {
        fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
            if *args.position == BlockPos::new(4, 98, 4) {
                self.0
                    .lock()
                    .unwrap()
                    .push(args.world.get_block_state_id(&BlockPos::new(4, 100, 4)));
            }
        }
    }

    let (_folder, mut world) = world().await;
    let observations = Arc::new(Mutex::new(Vec::new()));
    Arc::get_mut(&mut Arc::get_mut(&mut world).unwrap().block_registry)
        .unwrap()
        .register(ObserveNeighbors(observations.clone()));
    let pos = BlockPos::new(4, 100, 4);
    put(&world, pos.down(), Block::STONE.default_state.id);
    put(&world, pos.down().down(), Block::STONE.default_state.id);
    let mut lever = LeverLikeProperties::default(&Block::LEVER);
    lever.face = AttachFace::Floor;
    lever.powered = true;
    put(&world, pos, lever.to_state_id(&Block::LEVER));
    let source = CommandSource::dummy().with_world(world);
    assert_eq!(
        dispatcher()
            .execute_input("fill 4 100 4 4 100 4 gold_block destroy", &source)
            .unwrap(),
        1
    );
    let observations = observations.lock().unwrap();
    assert!(observations.contains(&BlockStateId::AIR));
    assert!(
        observations.contains(&Block::GOLD_BLOCK.default_state.id),
        "Java's powered-lever removal also notifies around its support after placement; the generic update at the fill position cannot reach two blocks below"
    );
}

async fn destroy_replacements_retain_new_entities_and_poi() {
    use pumpkin_nbt::compound::NbtCompound;

    let (_folder, world) = world().await;
    let source = CommandSource::dummy().with_world(world.clone());
    let dispatcher = dispatcher();
    for (x, replacement, expected_id) in [
        (4, "chest", "minecraft:chest"),
        (8, "spruce_sign", "minecraft:sign"),
    ] {
        let pos = BlockPos::new(x, 100, 4);
        put(&world, pos.down(), Block::STONE.default_state.id);
        world.set_block_state(
            &pos,
            Block::OAK_SIGN.default_state.id,
            BlockFlags::NOTIFY_ALL,
        );
        let old_entity = world.get_block_entity(&pos).expect("source sign entity");
        let mut front = NbtCompound::new();
        front.put_list(
            "messages",
            ["old sign text", "", "", ""]
                .into_iter()
                .map(|text| pumpkin_nbt::tag::NbtTag::String(text.into()))
                .collect(),
        );
        let mut nbt = NbtCompound::new();
        nbt.put_compound("front_text", front);
        old_entity.load_nbt(&nbt).unwrap();
        assert_eq!(
            dispatcher
                .execute_input(
                    &format!("fill {x} 100 4 {x} 100 4 {replacement} destroy"),
                    &source,
                )
                .unwrap(),
            1
        );
        let entity = world.get_block_entity(&pos).expect("replacement entity");
        assert!(!Arc::ptr_eq(&entity, &old_entity));
        if replacement == "chest" {
            assert!(entity.as_any().is::<ChestBlockEntity>());
            assert_eq!(world.get_block(&pos), &Block::CHEST);
        } else {
            let sign =
                crate::block::entities::sign::SignEntityRef::from_block_entity(entity.as_ref())
                    .unwrap();
            assert!(!sign.front_text().has_message(false));
            assert_eq!(world.get_block(&pos), &Block::SPRUCE_SIGN);
        }
        // The new handle must remain available to both packet and save paths.
        let mut saved = NbtCompound::new();
        entity.write_internal(&mut saved);
        assert_eq!(saved.get_string("id"), Some(expected_id));
        assert_eq!(saved.get_int("x"), Some(x));
        assert!(block_entity_from_nbt(&saved).is_some());
    }

    // A supported 2x3 portal, with POI entries for its actual interior blocks.
    for x in 11..=14 {
        for y in 99..=103 {
            let pos = BlockPos::new(x, y, 8);
            if x == 11 || x == 14 || y == 99 || y == 103 {
                put(&world, pos, Block::OBSIDIAN.default_state.id);
            } else {
                put(&world, pos, Block::NETHER_PORTAL.default_state.id);
            }
        }
    }
    let pos = BlockPos::new(12, 100, 8);
    assert_eq!(
        dispatcher
            .execute_input("fill 12 100 8 12 100 8 stonecutter destroy", &source)
            .unwrap(),
        1
    );
    assert_eq!(world.get_block(&pos), &Block::STONECUTTER);
    for chunk in &*world.level.loaded_chunks {
        world.level.bind_chunk_poi(chunk.value()).unwrap();
    }
    let records = world.level.poi_snapshot_in_square(pos, 0).unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|entry| entry.poi_type == "minecraft:mason")
            .map(|entry| BlockPos::new(entry.x, entry.y, entry.z))
            .collect::<Vec<_>>(),
        [pos]
    );
    assert!(
        !records
            .iter()
            .any(|entry| entry.poi_type == "minecraft:nether_portal")
    );
    // Observing the replacement must not publish an unsaved terrain snapshot.
    assert!(
        world
            .portal_poi
            .lock()
            .unwrap()
            .get_in_square(pos, 0, Some("minecraft:mason"))
            .unwrap()
            .is_empty()
    );
}

async fn gate_removal_notifies_past_the_front_block() {
    use crate::block::{BlockBehaviour, BlockMetadata, OnNeighborUpdateArgs};
    use pumpkin_data::BlockId;
    use pumpkin_data::block_properties::{
        ComparatorLikeProperties, HorizontalFacing, RepeaterLikeProperties,
    };
    use std::sync::Mutex;

    struct ObserveFront(Arc<Mutex<Vec<BlockStateId>>>);
    impl BlockMetadata for ObserveFront {
        fn ids() -> Box<[BlockId]> {
            [BlockId::STONE].into()
        }
    }
    impl BlockBehaviour for ObserveFront {
        fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
            if args.position.0.x == 8 && args.position.0.y == 100 {
                self.0
                    .lock()
                    .unwrap()
                    .push(
                        args.world
                            .get_block_state_id(&BlockPos::new(6, 100, args.position.0.z)),
                    );
            }
        }
    }

    let (_folder, mut world) = world().await;
    let observed = Arc::new(Mutex::new(Vec::new()));
    Arc::get_mut(&mut Arc::get_mut(&mut world).unwrap().block_registry)
        .unwrap()
        .register(ObserveFront(observed.clone()));
    let dispatcher = dispatcher();
    let source = CommandSource::dummy().with_world(world.clone());
    for (index, (block, destroy)) in [
        (&Block::REPEATER, false),
        (&Block::REPEATER, true),
        (&Block::COMPARATOR, false),
        (&Block::COMPARATOR, true),
    ]
    .into_iter()
    .enumerate()
    {
        let z = 2 + index as i32 * 3;
        let gate = BlockPos::new(6, 100, z);
        for pos in [
            gate.down(),
            BlockPos::new(7, 100, z),
            BlockPos::new(8, 100, z),
        ] {
            put(&world, pos, Block::STONE.default_state.id);
        }
        let state = if block == &Block::REPEATER {
            let mut props = RepeaterLikeProperties::default(block);
            props.facing = HorizontalFacing::West;
            props.powered = true;
            props.to_state_id(block)
        } else {
            let mut props = ComparatorLikeProperties::default(block);
            props.facing = HorizontalFacing::West;
            props.powered = true;
            let entity =
                Arc::new(crate::block::entities::comparator::ComparatorBlockEntity::new(gate));
            entity
                .output_signal
                .store(15, std::sync::atomic::Ordering::Relaxed);
            world.add_block_entity(entity);
            props.to_state_id(block)
        };
        put(&world, gate, state);
        assert_eq!(world.get_block_state_id(&gate), state);
        observed.lock().unwrap().clear();
        let command = if destroy {
            format!("fill 6 100 {z} 6 100 {z} gold_block destroy")
        } else {
            format!("setblock 6 100 {z} air")
        };
        assert_eq!(dispatcher.execute_input(&command, &source).unwrap(), 1);
        let observed = observed.lock().unwrap();
        assert!(
            observed.contains(&BlockStateId::AIR),
            "{command}: immediate removal must notify past the front block"
        );
        if destroy {
            assert!(
                observed.contains(&Block::GOLD_BLOCK.default_state.id),
                "{command}: deferred removal must use the original gate's facing"
            );
        }
    }
}

/// Advance only the fixture's real scheduled block queues and handlers. This
/// does not exercise world activation, entity ticking or terrain scheduling.
fn step_fixture_block_ticks(world: &Arc<World>, steps: usize) {
    use crate::block::OnScheduledTickArgs;
    for _ in 0..steps {
        let mut ticks = Vec::new();
        for x in 0..=1 {
            ticks.extend(
                world
                    .level
                    .read_chunk_sync(&Vector2::new(x, 0), |chunk| chunk.block_ticks.step_tick())
                    .unwrap(),
            );
        }
        ticks.sort_unstable();
        for tick in ticks {
            if world.get_block(&tick.position) == tick.value
                && let Some(behavior) = world.block_registry.get_pumpkin_block(tick.value.id)
            {
                behavior.on_scheduled_tick(OnScheduledTickArgs {
                    world,
                    block: tick.value,
                    position: &tick.position,
                });
            }
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep both hook endpoints, replacement controls and updates in one removal scenario."
)]
async fn tripwire_removal_updates_attached_hooks_and_retains_replacements() {
    use crate::block::{BlockBehaviour, BlockMetadata, OnNeighborUpdateArgs};
    use pumpkin_data::BlockId;
    use pumpkin_data::block_properties::{TripwireHookLikeProperties, TripwireLikeProperties};
    use std::sync::Mutex;

    struct ObserveHookSupport(Arc<Mutex<Vec<BlockStateId>>>);
    impl BlockMetadata for ObserveHookSupport {
        fn ids() -> Box<[BlockId]> {
            [BlockId::STONE].into()
        }
    }
    impl BlockBehaviour for ObserveHookSupport {
        fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
            if args.position.0.x == 3 && args.position.0.y == 100 {
                self.0
                    .lock()
                    .unwrap()
                    .push(
                        args.world
                            .get_block_state_id(&BlockPos::new(5, 100, args.position.0.z)),
                    );
            }
        }
    }

    let (_folder, mut world) = world().await;
    let observed = Arc::new(Mutex::new(Vec::new()));
    Arc::get_mut(&mut Arc::get_mut(&mut world).unwrap().block_registry)
        .unwrap()
        .register(ObserveHookSupport(observed.clone()));
    let dispatcher = dispatcher();
    let source = CommandSource::dummy().with_world(world.clone());
    for (index, (remove_hook, disarmed, destroy)) in [
        (false, false, false),
        (false, false, true),
        (false, true, false),
        (false, true, true),
        (true, false, false),
        (true, false, true),
    ]
    .into_iter()
    .enumerate()
    {
        let z = 2 + index as i32 * 2;
        for x in 3..=11 {
            put(
                &world,
                BlockPos::new(x, 99, z),
                Block::STONE.default_state.id,
            );
        }
        for x in [3, 4, 10] {
            put(
                &world,
                BlockPos::new(x, 100, z),
                Block::STONE.default_state.id,
            );
        }
        // Place hooks first and the middle string last. Actual command placement
        // must establish attachment; no attached=true fixture shortcut is used.
        for (x, block) in [
            (5, "tripwire_hook[facing=east]"),
            (9, "tripwire_hook[facing=west]"),
            (6, "tripwire"),
            (8, "tripwire"),
            (7, "tripwire"),
        ] {
            assert_eq!(
                dispatcher
                    .execute_input(&format!("setblock {x} 100 {z} {block}"), &source)
                    .unwrap(),
                1
            );
        }
        step_fixture_block_ticks(&world, 12);
        let west = BlockPos::new(5, 100, z);
        let east = BlockPos::new(9, 100, z);
        let middle = BlockPos::new(7, 100, z);
        for hook in [west, east] {
            assert_eq!(world.get_block(&hook), &Block::TRIPWIRE_HOOK);
            let props = TripwireHookLikeProperties::from_state_id(world.get_block_state_id(&hook));
            assert!(props.attached && !props.powered);
            assert!(!world.is_block_tick_scheduled(&hook, &Block::TRIPWIRE_HOOK));
        }
        if disarmed {
            let mut props =
                TripwireLikeProperties::from_state_id(world.get_block_state_id(&middle));
            props.disarmed = true;
            world.set_block_state(
                &middle,
                props.to_state_id(&Block::TRIPWIRE),
                BlockFlags::NOTIFY_LISTENERS,
            );
        }
        if remove_hook {
            // A powered attached line also exercises the removed hook's own
            // output notifications around its support, two blocks away.
            for hook in [west, east] {
                let mut props =
                    TripwireHookLikeProperties::from_state_id(world.get_block_state_id(&hook));
                props.powered = true;
                put(&world, hook, props.to_state_id(&Block::TRIPWIRE_HOOK));
            }
            let mut props =
                TripwireLikeProperties::from_state_id(world.get_block_state_id(&middle));
            props.powered = true;
            put(&world, middle, props.to_state_id(&Block::TRIPWIRE));
        }
        observed.lock().unwrap().clear();
        let x = if remove_hook { 5 } else { 7 };
        let command = if destroy {
            format!("fill {x} 100 {z} {x} 100 {z} gold_block destroy")
        } else {
            format!("setblock {x} 100 {z} air")
        };
        assert_eq!(dispatcher.execute_input(&command, &source).unwrap(), 1);
        let replacement = if destroy {
            Block::GOLD_BLOCK.default_state.id
        } else {
            BlockStateId::AIR
        };
        assert_eq!(
            world.get_block_state_id(&BlockPos::new(x, 100, z)),
            replacement
        );
        for hook in [west, east]
            .into_iter()
            .filter(|hook| *hook != BlockPos::new(x, 100, z))
        {
            let props = TripwireHookLikeProperties::from_state_id(world.get_block_state_id(&hook));
            if remove_hook || disarmed {
                assert!(
                    !props.attached && !props.powered,
                    "{command}: detach without a pulse"
                );
            } else {
                assert!(
                    props.attached && props.powered,
                    "{command}: armed string removal pulses before reset"
                );
            }
        }
        if remove_hook {
            let observed = observed.lock().unwrap();
            assert!(
                observed.contains(&BlockStateId::AIR),
                "{command}: notify the removed hook's support"
            );
            if destroy {
                assert!(observed.contains(&replacement));
            }
        } else {
            let scheduled = world
                .level
                .read_chunk_sync(&west.chunk_position(), |chunk| chunk.block_ticks.to_vec())
                .unwrap();
            assert!(scheduled.iter().any(|tick| tick.position == west
                && tick.value == &Block::TRIPWIRE_HOOK
                && tick.delay == 10));
        }
        step_fixture_block_ticks(&world, 12);
        assert_eq!(
            world.get_block_state_id(&BlockPos::new(x, 100, z)),
            replacement
        );
        for hook in [west, east]
            .into_iter()
            .filter(|hook| *hook != BlockPos::new(x, 100, z))
        {
            let props = TripwireHookLikeProperties::from_state_id(world.get_block_state_id(&hook));
            assert!(
                !props.attached && !props.powered,
                "{command}: line must be detached after reset"
            );
        }
        for wire in [BlockPos::new(6, 100, z), BlockPos::new(8, 100, z)] {
            assert!(
                !TripwireLikeProperties::from_state_id(world.get_block_state_id(&wire)).attached
            );
        }
    }
}

/// A regressed synchronous lock acquisition cannot be cancelled by a Tokio
/// timeout. Bound the actual dispatcher test in a child process instead.
#[test]
fn fill_destroy_dispatcher_releases_chunk_guards_and_preserves_drops() {
    use std::io::{Read, Seek, SeekFrom};
    const CHILD: &str = "FERRALOOM_FILL_DESTROY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                destroy_commands().await;
                destroy_callbacks_shapes_and_fluids().await;
                destroy_deferred_removal_preserves_lever_notifications().await;
                destroy_replacements_retain_new_entities_and_poi().await;
                gate_removal_notifies_past_the_front_block().await;
                tripwire_removal_updates_attached_hooks_and_retains_replacements().await;
            });
        return;
    }
    // A file captures both streams without a pipe that can fill while the
    // parent waits for a stuck child. Only bounded excerpts appear on failure.
    let mut output = tempfile::tempfile().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "command::commands::fill::tests::fill_destroy_dispatcher_releases_chunk_guards_and_preserves_drops", "--nocapture"])
        .env(CHILD, "1")
        .stdout(output.try_clone().unwrap())
        .stderr(output.try_clone().unwrap())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    output.seek(SeekFrom::Start(0)).unwrap();
    let mut start = Vec::new();
    (&mut output).take(4096).read_to_end(&mut start).unwrap();
    let start = String::from_utf8_lossy(&start);
    let tail_offset = output.metadata().unwrap().len().saturating_sub(65536);
    output.seek(SeekFrom::Start(tail_offset)).unwrap();
    let mut tail = Vec::new();
    #[expect(
        clippy::verbose_file_reads,
        reason = "Read the seeked tail through the retained child-output descriptor, not a reopened path."
    )]
    output.read_to_end(&mut tail).unwrap();
    let tail = String::from_utf8_lossy(&tail);
    assert!(
        status.is_some_and(|status| status.success()),
        "fill destroy child failed or exceeded 25 seconds: {status:?}\n{start}\n{tail}"
    );
    assert!(
        start.lines().any(|line| line == "running 1 test")
            && tail.contains("test result: ok. 1 passed; 0 failed;"),
        "the child must execute exactly one test, not merely exit successfully\n{start}\n{tail}"
    );
}

#[tokio::test]
async fn fill_other_modes_retain_keep_filter_outline_and_hollow_selection() {
    let (_folder, world) = world().await;
    let dispatcher = dispatcher();
    let source = CommandSource::dummy().with_world(world.clone());
    for y in 100..=102 {
        for z in 8..=10 {
            for x in 8..=10 {
                put(
                    &world,
                    BlockPos::new(x, y, z),
                    Block::STONE.default_state.id,
                );
            }
        }
    }
    assert_eq!(
        dispatcher
            .execute_input("fill 8 100 8 10 102 10 glass outline", &source)
            .unwrap(),
        26
    );
    assert_eq!(
        world.get_block_state_id(&BlockPos::new(9, 101, 9)),
        Block::STONE.default_state.id
    );
    assert_eq!(
        dispatcher
            .execute_input("fill 8 100 8 10 102 10 gold_block replace glass", &source)
            .unwrap(),
        26
    );
    assert_eq!(
        world.get_block_state_id(&BlockPos::new(9, 101, 9)),
        Block::STONE.default_state.id
    );
    assert_eq!(
        dispatcher
            .execute_input("fill 8 100 8 10 102 10 iron_block hollow", &source)
            .unwrap(),
        27
    );
    assert!(world.get_block_state(&BlockPos::new(9, 101, 9)).is_air());
    assert_eq!(
        dispatcher
            .execute_input("fill 8 100 8 10 102 10 diamond_block keep", &source)
            .unwrap(),
        1
    );
    assert_eq!(
        world.get_block_state_id(&BlockPos::new(9, 101, 9)),
        Block::DIAMOND_BLOCK.default_state.id
    );
    assert_eq!(
        world.get_block_state_id(&BlockPos::new(8, 100, 8)),
        Block::IRON_BLOCK.default_state.id
    );
}
