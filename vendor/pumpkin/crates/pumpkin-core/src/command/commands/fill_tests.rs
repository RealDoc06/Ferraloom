use super::*;
use crate::{block::entities::chest::ChestBlockEntity, world::World};
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
    let folder = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        folder.path().into(),
        0,
        Dimension::OVERWORLD,
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
        Dimension::OVERWORLD,
        crate::block::registry::default_registry(),
        Weak::new(),
        Arc::default(),
    ));
    (folder, world)
}

fn dispatcher() -> CommandDispatcher {
    let mut dispatcher = CommandDispatcher::new();
    register(&mut dispatcher, &PermissionRegistry::default());
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

/// A regressed synchronous lock acquisition cannot be cancelled by a Tokio
/// timeout. Bound the actual dispatcher test in a child process instead.
#[test]
fn fill_destroy_dispatcher_releases_chunk_guards_and_preserves_drops() {
    const CHILD: &str = "FERRALOOM_FILL_DESTROY_CHILD";
    if std::env::var_os(CHILD).is_some() {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(destroy_commands());
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "command::commands::fill::tests::fill_destroy_dispatcher_releases_chunk_guards_and_preserves_drops", "--nocapture"])
        .env(CHILD, "1")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "fill destroy child failed: {status}");
            return;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("fill destroy command did not complete within 25 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
