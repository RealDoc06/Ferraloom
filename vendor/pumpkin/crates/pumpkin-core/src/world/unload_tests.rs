//! Exercise terrain eviction through the live scheduler and read fresh region
//! files afterward. No loaded chunk is removed or inserted by these fixtures.
use super::*;
use crate::block::entities::{
    chest::ChestBlockEntity, furnace::FurnaceBlockEntity, hopper::HopperBlockEntity,
};
use pumpkin_config::world::LevelConfig;
use pumpkin_data::{
    block_properties::{FacingHopper, FurnaceLikeProperties, HopperLikeProperties},
    item::Item,
};
use pumpkin_util::world_seed::Seed;
use pumpkin_world::{
    chunk::{
        ChunkLight,
        format::{LightContainer, anvil::AnvilChunkFile},
        io::{FileIO, LoadedData, file_manager::ChunkFileManager},
    },
    chunk_system::ChunkLoading,
};
use std::time::Duration;

async fn scheduler_world() -> (tempfile::TempDir, Arc<World>) {
    scheduler_world_with_east_halo(7).await
}

async fn scheduler_world_with_east_halo(max_x: i32) -> (tempfile::TempDir, Arc<World>) {
    let folder = tempfile::tempdir().unwrap();
    let seed = Level::from_root_folder(
        &LevelConfig::default(),
        folder.path().into(),
        0,
        Dimension::OVERWORLD,
    );
    seed.shutdown().await.unwrap();
    // Force/activation tickets reach radius seven. FULL disk fixtures satisfy
    // every holder without starting terrain generation on the test machine.
    let mut chunks = Vec::new();
    for x in -7..=max_x {
        for z in -7..=7 {
            let chunk = ChunkData::empty_sync(x, z);
            *chunk.light_engine.lock().unwrap() = ChunkLight {
                sky_light: vec![LightContainer::new_empty(0); chunk.section.count]
                    .into_boxed_slice(),
                block_light: vec![LightContainer::new_empty(0); chunk.section.count]
                    .into_boxed_slice(),
            };
            chunk.mark_dirty(true);
            chunks.push((Vector2::new(x, z), chunk));
        }
    }
    seed.write_chunks(chunks).await;
    seed.chunk_saver.flush().await.unwrap();
    drop(seed);

    let level = Level::from_root_folder(
        &LevelConfig::default(),
        folder.path().into(),
        0,
        Dimension::OVERWORLD,
    );
    let world = Arc::new(World::load(
        level.clone(),
        Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
        Dimension::OVERWORLD,
        crate::block::registry::default_registry(),
        Weak::new(),
        Arc::new(std::sync::Mutex::new(Scoreboard::new())),
    ));
    let portal: Arc<dyn WorldPortalExt> = Arc::new(WorldPortal(world.clone()));
    level.world_portal.store(Arc::new(Some(portal)));
    (folder, world)
}

fn ticket(world: &World, pos: Vector2<i32>, add: bool) {
    let mut loading = world.level.chunk_loading.lock().unwrap();
    if add {
        loading.add_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
    } else {
        loading.remove_ticket(pos, ChunkLoading::FULL_CHUNK_LEVEL);
    }
    loading.send_change();
}

async fn published(world: &Arc<World>, pos: Vector2<i32>) -> SyncChunk {
    tokio::time::timeout(
        Duration::from_secs(10),
        world.level.get_or_fetch_chunk(pos, Clone::clone),
    )
    .await
    .expect("FULL disk chunk must publish within the bound")
    .expect("FULL disk chunk must be readable")
}

async fn unloaded(world: &World, pos: Vector2<i32>) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while world.level.is_chunk_loaded(&pos) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("real scheduler must evict the unticketed terrain");
    world.level.save_and_flush().await.unwrap();
    assert!(!world.block_entities.contains_key(&pos));
}

async fn saved_chest(world: &World, position: BlockPos) -> ChestBlockEntity {
    let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(
        pumpkin_config::chunk::AnvilChunkConfig::default(),
    );
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    reader
        .fetch_chunks(
            &world.level.level_folder,
            &[position.chunk_position()],
            send,
        )
        .await;
    let Some(LoadedData::Loaded(chunk)) = receive.recv().await else {
        panic!("evicted chunk must exist in a fresh region reader");
    };
    let nbt = chunk
        .pending_block_entities
        .lock()
        .unwrap()
        .get(&position)
        .unwrap()
        .clone();
    ChestBlockEntity::from_nbt(&nbt, position)
}

fn chest(world: &World, position: BlockPos, count: u8) -> Arc<ChestBlockEntity> {
    let relative = position.chunk_relative_position();
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(
                relative.x as usize,
                relative.y,
                relative.z as usize,
                Block::CHEST.default_state.id,
            );
        })
        .unwrap();
    let chest = Arc::new(ChestBlockEntity::new(position));
    if count != 0 {
        chest.set_stack(0, ItemStack::new(count, &Item::DIAMOND));
    }
    world.add_block_entity(chest.clone());
    chest
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_saves_hopper_push_into_unwatched_full_halo() {
    let (_folder, world) = scheduler_world().await;
    let center = Vector2::new(0, 0);
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(center);
        loading.send_change();
    };
    world.forced_chunks.lock().unwrap().insert(center);
    world.update_active_chunks();
    let source = BlockPos::new(31, 100, 1);
    let target = BlockPos::new(32, 100, 1);
    published(&world, source.chunk_position()).await;
    published(&world, target.chunk_position()).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            world.update_active_chunks();
            if world
                .block_ticking_chunks
                .read()
                .unwrap()
                .contains(&source.chunk_position())
                && world
                    .level
                    .get_entity_chunk_sync(&source.chunk_position())
                    .is_some_and(|chunk| chunk.live.load(Relaxed))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!world.level.is_chunk_watched(&target.chunk_position()));
    assert!(
        !world
            .block_ticking_chunks
            .read()
            .unwrap()
            .contains(&target.chunk_position())
    );
    let relative = target.chunk_relative_position();
    world
        .level
        .read_chunk_sync(&target.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(
                relative.x as usize,
                relative.y,
                relative.z as usize,
                Block::CHEST.default_state.id,
            );
        });
    let mut nbt = NbtCompound::new();
    ChestBlockEntity::new(target).write_internal(&mut nbt);
    world.add_block_entity_nbt(target, &nbt);
    assert!(!world.block_entities.contains_key(&target.chunk_position()));
    let mut properties = HopperLikeProperties::default(&Block::HOPPER);
    properties.facing = FacingHopper::East;
    let relative = source.chunk_relative_position();
    world
        .level
        .read_chunk_sync(&source.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(
                relative.x as usize,
                relative.y,
                relative.z as usize,
                properties.to_state_id(&Block::HOPPER),
            );
        });
    let hopper = Arc::new(HopperBlockEntity::new(source, FacingHopper::East));
    hopper.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
    world.add_block_entity(hopper.clone());
    world.tick_block_entities(&tokio::runtime::Handle::current());
    assert_eq!(hopper.get_stack(0).item_count, 2);
    let destination = world
        .get_block_entity(&target)
        .unwrap()
        .get_inventory()
        .unwrap();
    assert_eq!(destination.get_stack(0).item_count, 1);
    drop(destination);
    drop(hopper);

    world.forced_chunks.lock().unwrap().clear();
    world.update_active_chunks();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.remove_force_ticket(center);
        loading.send_change();
    };
    unloaded(&world, target.chunk_position()).await;
    assert_eq!(saved_chest(&world, target).await.get_stack(0).item_count, 1);
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_snapshots_be_even_when_existing_only_spectator_watch_remains() {
    let (_folder, world) = scheduler_world().await;
    let pos = Vector2::new(0, 0);
    let position = BlockPos::new(1, 100, 1);
    ticket(&world, pos, true);
    published(&world, pos).await;
    world.update_chunk_watches(&[pos], &[]).await;
    drop(chest(&world, position, 5));
    ticket(&world, pos, false);
    unloaded(&world, pos).await;
    assert!(
        world.level.is_chunk_watched(&pos),
        "ExistingOnly view owners retain watches without tickets"
    );
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        5
    );
    world.update_chunk_watches(&[], &[pos]).await;
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_retains_external_inventory_through_unwatch_and_late_command_write() {
    let (_folder, world) = scheduler_world().await;
    let pos = Vector2::new(0, 0);
    let position = BlockPos::new(1, 100, 1);
    ticket(&world, pos, true);
    published(&world, pos).await;
    world.update_chunk_watches(&[pos], &[]).await;
    let held = chest(&world, position, 2);
    world.update_chunk_watches(&[], &[pos]).await;
    ticket(&world, pos, false);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(world.level.is_chunk_loaded(&pos));
    assert!(world.block_entities.contains_key(&pos));
    held.set_stack(0, ItemStack::new(7, &Item::DIAMOND));
    drop(held);
    unloaded(&world, pos).await;
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        7
    );
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_worker_refuses_pending_request_when_full_ticket_already_returned() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (_folder, world) = scheduler_world().await;
        let pos = Vector2::new(0, 0);
        ticket(&world, pos, true);
        let chunk = published(&world, pos).await;
        drop(chest(&world, BlockPos::new(1, 100, 1), 2));
        let lifecycle = world.entity_lifecycle.lock().await;
        {
            let mut unload = world.level.chunk_unload.access();
            // Model the worker running before the scheduler consumes a renewed
            // FULL ticket. No external BE handle can block retirement instead.
            assert!(!unload.prepare_or_defer(pos, &chunk));
            assert!(unload.is_requested(pos, &chunk));
        };
        assert!(!world.retire_block_entities(pos, Some(&chunk), &mut Vec::new()));
        assert!(world.block_entities.contains_key(&pos));
        assert!(!world.level.chunk_unload.access().is_requested(pos, &chunk));
        drop(lifecycle);
        world.shutdown().await.unwrap();
    })
    .await
    .expect("direct renewed-ticket snapshot check must finish within 15 seconds");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_rechecks_renewed_ticket_and_invalidated_snapshot_before_eviction() {
    let (_folder, world) = scheduler_world().await;
    let pos = Vector2::new(0, 0);
    let position = BlockPos::new(1, 100, 1);
    ticket(&world, pos, true);
    published(&world, pos).await;
    drop(chest(&world, position, 2));
    let lifecycle = world.entity_lifecycle.lock().await;
    ticket(&world, pos, false);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !world
            .level
            .chunk_unload
            .access()
            .requests()
            .iter()
            .any(|(requested, _)| *requested == pos)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("scheduler must request a World snapshot before unpublishing");
    ticket(&world, pos, true);
    drop(lifecycle);
    tokio::time::timeout(Duration::from_secs(5), async {
        while world.unload_snapshot_worker.load(Ordering::Acquire)
            || world
                .level
                .chunk_unload
                .access()
                .requests()
                .iter()
                .any(|(requested, _)| *requested == pos)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        world.level.is_chunk_loaded(&pos),
        "renewed FULL ticket cancels the pending unload"
    );
    assert!(
        world.block_entities.contains_key(&pos),
        "renewal keeps live block entities in the map"
    );
    let entity = world.get_block_entity(&position).unwrap();
    entity
        .clone()
        .get_inventory()
        .unwrap()
        .set_stack(0, ItemStack::new(9, &Item::DIAMOND));
    drop(entity);
    ticket(&world, pos, false);
    unloaded(&world, pos).await;
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        9
    );
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renewed_block_ticking_furnace_keeps_cooking_when_another_chunk_requests_unload() {
    // The second FULL ticket needs its own disk halo, outside the renewed
    // simulation owner's radius, so its real unload can trigger another pass.
    let (_folder, world) = scheduler_world_with_east_halo(12).await;
    let pos = Vector2::new(0, 0);
    let other = Vector2::new(7, 0);
    let position = BlockPos::new(1, 100, 1);
    ticket(&world, pos, true);
    ticket(&world, other, true);
    let chunk = published(&world, pos).await;
    published(&world, other).await;
    let mut properties = FurnaceLikeProperties::default(&Block::FURNACE);
    properties.lit = true;
    chunk.set_block_absolute_y(1, position.0.y, 1, properties.to_state_id(&Block::FURNACE));
    let furnace = Arc::new(FurnaceBlockEntity::new(position));
    furnace.set_stack(0, ItemStack::new(3, &Item::RAW_IRON));
    furnace.set_stack(1, ItemStack::new(1, &Item::COAL));
    furnace.cooking_total_time.store(200, Ordering::Relaxed);
    furnace.cooking_time_spent.store(20, Ordering::Relaxed);
    furnace.lit_time_remaining.store(1000, Ordering::Relaxed);
    furnace.lit_total_time.store(1000, Ordering::Relaxed);
    furnace.comparator_dirty.store(false, Ordering::Relaxed);
    world.add_block_entity(furnace.clone());
    drop(chest(&world, BlockPos::new(113, 100, 1), 1));

    // Keep an inventory handle while the first owner leaves. The worker must
    // defer this request without detaching the live furnace.
    ticket(&world, pos, false);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !world
            .level
            .chunk_unload
            .access()
            .requests()
            .iter()
            .any(|(requested, _)| *requested == pos)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the scheduler must request the initial furnace snapshot");
    ticket(&world, pos, true);
    world.forced_chunks.lock().unwrap().insert(pos);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            world.update_active_chunks();
            let ready = {
                let tracker = world.active_chunk_tracker.lock().unwrap();
                tracker.loaded_block_ticking_chunks == tracker.requested_block_chunks
                    && world.block_ticking_chunks.read().unwrap().len() == 9
            };
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("renewed furnace must become block ticking");
    for _ in 0..5 {
        world.tick_block_entities(&tokio::runtime::Handle::current());
    }
    assert_eq!(furnace.cooking_time_spent.load(Ordering::Relaxed), 25);
    drop(furnace);

    // Do not call get_block_entity or update_active_chunks before inspecting
    // progress: those could conceal a lost live map by migrating saved NBT.
    ticket(&world, other, false);
    unloaded(&world, other).await;
    for _ in 0..5 {
        world.tick_block_entities(&tokio::runtime::Handle::current());
    }
    let entities = world
        .block_entities
        .get(&pos)
        .expect("renewed active furnace must stay live");
    let furnace = entities
        .get(&position)
        .unwrap()
        .as_any()
        .downcast_ref::<FurnaceBlockEntity>()
        .unwrap();
    assert_eq!(furnace.cooking_time_spent.load(Ordering::Relaxed), 30);
    assert_eq!(furnace.get_stack(0).item_count, 3);
    drop(entities);
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduler_defers_save_off_terrain_until_save_on_or_shutdown() {
    let (_folder, world) = scheduler_world().await;
    let pos = Vector2::new(0, 0);
    let position = BlockPos::new(1, 100, 1);
    ticket(&world, pos, true);
    published(&world, pos).await;
    let held = chest(&world, position, 2);
    world.save().await.unwrap();
    world.level.save_and_flush().await.unwrap();
    world.level.save_enabled.store(false, Ordering::Release);
    held.set_stack(0, ItemStack::new(5, &Item::DIAMOND));
    drop(held);
    ticket(&world, pos, false);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(world.level.is_chunk_loaded(&pos));
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        2
    );
    assert_eq!(
        world
            .get_block_entity(&position)
            .unwrap()
            .get_inventory()
            .unwrap()
            .get_stack(0)
            .item_count,
        5
    );
    // Explicit saves override save-off, while ordinary terrain eviction waits.
    world.save().await.unwrap();
    world.level.save_and_flush().await.unwrap();
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        5
    );
    world
        .get_block_entity(&position)
        .unwrap()
        .get_inventory()
        .unwrap()
        .set_stack(0, ItemStack::new(7, &Item::DIAMOND));
    world.level.save_enabled.store(true, Ordering::Release);
    world.level.should_unload.store(true, Ordering::Release);
    world.level.level_channel.notify();
    unloaded(&world, pos).await;
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        7
    );

    ticket(&world, pos, true);
    published(&world, pos).await;
    drop(chest(&world, position, 11));
    world.level.save_enabled.store(false, Ordering::Release);
    ticket(&world, pos, false);
    world.shutdown().await.unwrap();
    assert_eq!(
        saved_chest(&world, position).await.get_stack(0).item_count,
        11
    );
}
