//! Source-only controls through real Level containers, chunk NBT and save IO.
//! All tests in this module are UNRUN until ROOT's builder validation.

use super::{SavedTick, ScheduledTick, TickPriority};
use crate::{
    chunk::{
        ChunkData, ChunkSerializingError,
        format::anvil::{AnvilChunkFile, SingleChunkDataSerializer},
        io::{Dirtiable, FileIO, LoadedData, file_manager::ChunkFileManager},
    },
    level::{Level, SyncChunk},
    poi::PoiReconcileError,
};
use pumpkin_config::{chunk::AnvilChunkConfig, world::LevelConfig};
use pumpkin_data::{Block, dimension::Dimension, fluid::Fluid};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use rustc_hash::FxHashSet;
use std::sync::{Arc, atomic::Ordering};

async fn fixture() -> (tempfile::TempDir, Arc<Level>) {
    let folder = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        folder.path().to_owned(),
        0,
        Dimension::OVERWORLD,
    );
    // The existing standalone fixture convention: automatic workers have ended,
    // while the real map, schedulers and backend APIs remain under test control.
    level.shutdown().await.unwrap();
    (folder, level)
}

fn publish(level: &Level, x: i32) -> SyncChunk {
    let chunk = ChunkData::empty_sync(x, 0);
    chunk.post_processing_ready.store(true, Ordering::Relaxed);
    level.bind_chunk_poi(&chunk).unwrap();
    level
        .loaded_chunks
        .insert(Vector2::new(x, 0), chunk.clone());
    chunk
}

fn tick(x: i32, trigger: i64, order: i64) -> ScheduledTick<&'static Block> {
    ScheduledTick {
        position: BlockPos::new(x, 64, 0),
        value: &Block::STONE,
        trigger_tick: trigger,
        priority: TickPriority::Normal,
        sub_tick_order: order,
    }
}

#[tokio::test]
async fn saved_pending_real_chunk_nbt_keeps_signed_delays_duplicates_and_one_unpack() {
    let (_folder, level) = fixture().await;
    level.set_tick_game_time(9_000_000);
    let mut source = ChunkData::empty(0, 0);
    let saved = [
        SavedTick {
            position: BlockPos::new(0, 64, 0),
            value: &Block::STONE,
            delay: -7,
            priority: TickPriority::High,
        },
        SavedTick {
            position: BlockPos::new(1, 64, 0),
            value: &Block::DIRT,
            delay: 600,
            priority: TickPriority::Low,
        },
        SavedTick {
            position: BlockPos::new(0, 64, 0),
            value: &Block::STONE,
            delay: -3,
            priority: TickPriority::Normal,
        },
    ];
    source.block_ticks = saved.into_iter().collect();
    source.fluid_ticks = [
        SavedTick {
            position: BlockPos::new(2, 64, 0),
            value: &Fluid::WATER,
            delay: -5,
            priority: TickPriority::High,
        },
        SavedTick {
            position: BlockPos::new(3, 64, 0),
            value: &Fluid::LAVA,
            delay: 700,
            priority: TickPriority::Low,
        },
    ]
    .into_iter()
    .collect();
    // Unbound pending-only chunks pack verbatim, through the actual chunk codec.
    let bytes = source.to_bytes().unwrap();
    let restored = Arc::new(ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap());
    let delays = |chunk: &ChunkData| {
        chunk
            .block_ticks
            .pack_at(123)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>()
    };
    assert_eq!(delays(&source), vec![-7, 600, -3]);
    assert_eq!(delays(&restored), vec![-7, 600, -3]);
    assert_eq!(
        restored
            .fluid_ticks
            .pack_at(-500)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![-5, 700]
    );
    assert!(restored.block_ticks.get_all().is_empty());
    assert!(
        restored
            .block_ticks
            .is_scheduled(BlockPos::new(0, 64, 0), &Block::STONE)
    );
    restored
        .post_processing_ready
        .store(true, Ordering::Relaxed);
    level.bind_chunk_poi(&restored).unwrap();
    level
        .loaded_chunks
        .insert(Vector2::new(0, 0), restored.clone());
    let target = publish(&level, 1);
    level.schedule_block_tick(&Block::STONE, BlockPos::new(0, 64, 0), 1, TickPriority::Low);
    let min = BlockPos::new(0, 64, 0);
    let max = BlockPos::new(3, 64, 0);
    level.copy_block_ticks_from(&level, &min, &max, &BlockPos::new(16, 0, 0));
    assert!(target.block_ticks.get_all().is_empty());
    assert_eq!(delays(&restored), vec![-7, 600, -3]);

    level.unpack_chunk_ticks(Vector2::new(0, 0), 9_000_000);
    let mut queued = restored.block_ticks.get_all();
    queued.sort_by_key(|tick| tick.sub_tick_order);
    assert_eq!(
        queued
            .iter()
            .map(|tick| (tick.trigger_tick, tick.sub_tick_order))
            .collect::<Vec<_>>(),
        vec![(8_999_993, -3), (9_000_600, -2), (8_999_997, -1)]
    );
    assert_eq!(
        restored
            .fluid_ticks
            .pack_at(9_000_000)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![-5, 700]
    );
    level.unpack_chunk_ticks(Vector2::new(0, 0), 10_000_000);
    assert_eq!(restored.block_ticks.get_all().len(), 3);
    assert_eq!(
        restored
            .block_ticks
            .pack_at(9_000_000)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![-7, 600, -3]
    );
    // Absolute queues round-trip via capture's shared game clock too.
    let queued_bytes = restored.to_bytes().unwrap();
    let decoded = ChunkData::from_bytes(&queued_bytes, Vector2::new(0, 0)).unwrap();
    assert_eq!(delays(&decoded), vec![-7, 600, -3]);
    assert_eq!(
        decoded
            .fluid_ticks
            .pack_at(0)
            .iter()
            .map(|tick| tick.delay)
            .collect::<Vec<_>>(),
        vec![-5, 700]
    );

    let eligible = FxHashSet::from_iter([Vector2::new(0, 0)]);
    level.collect_block_ticks(8_999_994, 65536, &eligible);
    assert_eq!(level.pop_block_tick().unwrap().trigger_tick, 8_999_993);
    assert!(level.pop_block_tick().is_none());
    // Poll removes the saved duplicate's key, just like LevelChunkTicks.poll.
    assert!(!level.is_block_tick_scheduled(&min, &Block::STONE));
    assert_eq!(restored.block_ticks.get_all().len(), 2);
    level.finish_block_ticks();
}

#[tokio::test]
async fn unbound_absolute_chunk_encoding_refuses_and_wrapping_clock_is_explicit() {
    let (_folder, level) = fixture().await;
    let chunk = ChunkData::empty_sync(0, 0);
    level
        .loaded_chunks
        .insert(Vector2::new(0, 0), chunk.clone());
    level.set_tick_game_time(i64::MAX);
    level.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(0, 64, 0),
        1,
        TickPriority::High,
    );
    assert_eq!(chunk.block_ticks.get_all()[0].trigger_tick, i64::MIN);
    assert!(matches!(
        chunk.to_bytes(),
        Err(ChunkSerializingError::Poi(PoiReconcileError::UnboundChunk))
    ));
    assert!(matches!(
        chunk.capture_for_save(),
        Err(ChunkSerializingError::Poi(PoiReconcileError::UnboundChunk))
    ));
    level.bind_chunk_poi(&chunk).unwrap();
    let bytes = chunk.to_bytes().unwrap();
    let decoded = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).unwrap();
    assert_eq!(decoded.block_ticks.pack_at(0)[0].delay, 1);
}

#[tokio::test]
async fn absolute_chunk_head_drain_interleaves_intra_order_and_keeps_one_phase_owner() {
    let (_folder, level) = fixture().await;
    let a = publish(&level, 0);
    let b = publish(&level, 1);
    let mut entries = [
        tick(0, 1, 0),
        tick(1, 2, 100),
        tick(2, 3, -5),
        tick(16, 4, 10),
        tick(17, 4, 0),
    ];
    entries[1].priority = TickPriority::Low;
    entries[2].priority = TickPriority::High;
    entries[3].priority = TickPriority::High;
    entries[4].priority = TickPriority::Low;
    for entry in entries {
        assert!(level.schedule_block_tick_absolute(entry));
    }
    let eligible = FxHashSet::from_iter([Vector2::new(0, 0), Vector2::new(1, 0)]);
    level.collect_block_ticks(10, 3, &eligible);
    assert_eq!(a.block_tick_phase_owners.load(Ordering::Acquire), 1);
    assert_eq!(b.block_tick_phase_owners.load(Ordering::Acquire), 1);
    let due: Vec<_> = std::iter::from_fn(|| level.pop_block_tick())
        .map(|tick| tick.position.0.x)
        .collect();
    assert_eq!(due, vec![16, 0, 17]);
    assert_eq!(a.block_ticks.get_all().len(), 2);
    level.finish_block_ticks();
    assert_eq!(a.block_tick_phase_owners.load(Ordering::Acquire), 0);
    assert_eq!(b.block_tick_phase_owners.load(Ordering::Acquire), 0);
    level.collect_block_ticks(10, 65536, &eligible);
    let due: Vec<_> = std::iter::from_fn(|| level.pop_block_tick())
        .map(|tick| tick.position.0.x)
        .collect();
    assert_eq!(due, vec![1, 2]);
    level.finish_block_ticks();
    assert!(
        !level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(0, 0))
    );
    assert!(
        !level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(1, 0))
    );
    let mut other_type = tick(0, 20, 7);
    other_type.value = &Block::DIRT;
    for entry in [tick(0, 20, 7), tick(1, 20, 7), other_type] {
        assert!(level.schedule_block_tick_absolute(entry));
    }
    level.collect_block_ticks(20, 65536, &eligible);
    let identities: FxHashSet<_> = std::iter::from_fn(|| level.pop_block_tick())
        .map(|tick| (tick.position.0.x, tick.value.id))
        .collect();
    assert_eq!(
        identities,
        FxHashSet::from_iter([
            (0, Block::STONE.id),
            (1, Block::STONE.id),
            (0, Block::DIRT.id)
        ])
    );
    level.finish_block_ticks();
}

#[tokio::test]
async fn fluid_only_index_survives_block_drain_and_fluid_budget_uses_absolute_time() {
    let (_folder, level) = fixture().await;
    let block_chunk = publish(&level, 0);
    let fluid_chunk = publish(&level, 1);
    level.set_tick_game_time(500);
    level.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(0, 64, 0),
        0,
        TickPriority::Normal,
    );
    level.schedule_fluid_tick(&Fluid::WATER, BlockPos::new(1, 64, 0), 0, TickPriority::Low);
    level.schedule_fluid_tick(
        &Fluid::WATER,
        BlockPos::new(16, 64, 0),
        0,
        TickPriority::High,
    );
    level.schedule_fluid_tick(
        &Fluid::LAVA,
        BlockPos::new(17, 64, 0),
        255,
        TickPriority::Normal,
    );
    let eligible = FxHashSet::from_iter([Vector2::new(0, 0), Vector2::new(1, 0)]);
    level.collect_block_ticks(500, 65536, &eligible);
    assert!(level.pop_block_tick().is_some());
    assert!(level.pop_block_tick().is_none());
    assert!(
        level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(0, 0))
    );
    assert!(
        level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(1, 0))
    );
    level.finish_block_ticks();
    assert_eq!(
        block_chunk.block_tick_phase_owners.load(Ordering::Acquire),
        0
    );
    let due = level.collect_fluid_ticks(500, 1, &eligible);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].position.0.x, 16);
    let due = level.collect_fluid_ticks(500, 65536, &eligible);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].position.0.x, 1);
    assert!(
        !level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(0, 0))
    );
    assert!(
        level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(1, 0))
    );
    assert!(level.collect_fluid_ticks(754, 65536, &eligible).is_empty());
    assert_eq!(
        level.collect_fluid_ticks(755, 65536, &eligible)[0].value.id,
        Fluid::LAVA.id
    );
    assert!(!fluid_chunk.fluid_ticks.has_ticks());
    assert!(
        !level
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(1, 0))
    );
}

#[tokio::test]
async fn copy_partitions_history_and_queues_before_dedup_and_inserts_only_current_owner() {
    let (_source_folder, source) = fixture().await;
    let (_target_folder, target) = fixture().await;
    let origin = publish(&source, 0);
    let displaced = publish(&target, 2);
    source.set_tick_game_time(1_000);
    target.set_tick_game_time(42);
    for entry in [tick(0, 1_000, -10), tick(1, 1_000, 5), tick(2, 2_000, 20)] {
        assert!(source.schedule_block_tick_absolute(entry));
    }
    source.schedule_fluid_tick(
        &Fluid::WATER,
        BlockPos::new(3, 64, 0),
        0,
        TickPriority::Normal,
    );
    assert!(target.schedule_block_tick_absolute(tick(32, 777, -800)));
    let mut replacement = ChunkData::empty(2, 0);
    replacement.block_ticks = [SavedTick {
        position: BlockPos::new(32, 64, 0),
        value: &Block::STONE,
        delay: 1,
        priority: TickPriority::Low,
    }]
    .into_iter()
    .collect();
    replacement
        .post_processing_ready
        .store(true, Ordering::Relaxed);
    let replacement = Arc::new(replacement);
    target.bind_chunk_poi(&replacement).unwrap();
    target
        .loaded_chunks
        .insert(Vector2::new(2, 0), replacement.clone());
    source.collect_block_ticks(1_000, 2, &FxHashSet::from_iter([Vector2::new(0, 0)]));
    assert_eq!(source.pop_block_tick().unwrap().position.0.x, 0);
    assert!(!source.is_block_tick_scheduled(&BlockPos::new(0, 64, 0), &Block::STONE));
    assert!(source.schedule_block_tick_absolute(tick(0, 3_000, 100)));
    let min = BlockPos::new(0, 64, 0);
    let max = BlockPos::new(3, 64, 0);
    let snapshot = source.snapshot_block_ticks(&min, &max);
    assert_eq!(
        snapshot
            .iter()
            .map(|tick| (tick.position.0.x, tick.sub_tick_order))
            .collect::<Vec<_>>(),
        vec![(0, -10), (1, 5), (2, 20), (0, 100)]
    );
    target.copy_block_ticks_from(&source, &min, &max, &BlockPos::new(32, 0, 0));
    let mut copied = replacement.block_ticks.get_all();
    copied.sort_by_key(|tick| tick.position.0.x);
    assert_eq!(
        copied
            .iter()
            .map(|tick| (tick.position.0.x, tick.trigger_tick, tick.sub_tick_order))
            .collect::<Vec<_>>(),
        vec![(33, 1_000, 116), (34, 2_000, 131)]
    );
    assert_eq!(replacement.block_ticks.pack_at(42)[0].delay, 1);
    assert_eq!(displaced.block_ticks.get_all().len(), 1);
    assert_eq!(displaced.block_ticks.get_all()[0].trigger_tick, 777);
    assert!(replacement.fluid_ticks.get_all().is_empty());
    assert!(origin.fluid_ticks.has_ticks());
    assert!(
        target
            .chunks_with_scheduled_ticks
            .contains(&Vector2::new(2, 0))
    );
    target.copy_block_ticks_from(&source, &min, &max, &BlockPos::new(32, 0, 0));
    assert_eq!(replacement.block_ticks.get_all().len(), 2);
    target.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(35, 64, 0),
        1,
        TickPriority::Normal,
    );
    assert_eq!(
        replacement
            .block_ticks
            .get_all()
            .iter()
            .find(|tick| tick.position.0.x == 35)
            .unwrap()
            .sub_tick_order,
        0
    );
    assert!(!target.schedule_block_tick_absolute(tick(200, 10, 0)));
    assert_eq!(source.pop_block_tick().unwrap().position.0.x, 1);
    assert!(source.pop_block_tick().is_none());
    source.finish_block_ticks();
    assert_eq!(source.snapshot_block_ticks(&min, &max).len(), 2);
    // Same-world overlap snapshots fully before any insertion; queued dedup wins.
    source.copy_block_ticks_from(&source, &min, &max, &BlockPos::new(1, 0, 0));
    assert_eq!(origin.block_ticks.get_all().len(), 4);
    // Existing replacement paths may displace a phase owner. Its Arc is only a
    // retirement pin; a callback's new schedule must resolve the current map.
    source.set_tick_game_time(4_000);
    source.collect_block_ticks(4_000, 1, &FxHashSet::from_iter([Vector2::new(0, 0)]));
    assert!(source.pop_block_tick().is_some());
    assert!(source.pop_block_tick().is_none());
    assert_eq!(origin.block_tick_phase_owners.load(Ordering::Acquire), 1);
    let remaining = origin.block_ticks.get_all().len();
    let current = publish(&source, 0);
    source.schedule_block_tick(
        &Block::STONE,
        BlockPos::new(7, 64, 0),
        1,
        TickPriority::Normal,
    );
    assert_eq!(current.block_ticks.get_all()[0].trigger_tick, 4_001);
    assert_eq!(origin.block_ticks.get_all().len(), remaining);
    assert!(
        !origin
            .block_ticks
            .is_scheduled(BlockPos::new(7, 64, 0), &Block::STONE)
    );
    source.finish_block_ticks();
    assert_eq!(origin.block_tick_phase_owners.load(Ordering::Acquire), 0);
    assert_eq!(current.block_tick_phase_owners.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn tick_only_dirty_change_recaptures_retained_source_without_poi_revision_change() {
    let (_folder, level) = fixture().await;
    let chunk = publish(&level, 0);
    level.set_tick_game_time(1_000);
    let position = Vector2::new(0, 0);
    let stamp = level.prepare_chunk_poi(&chunk).unwrap();
    let writer = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
    writer.watch_chunks(&level.level_folder, &[position]).await;
    writer
        .save_chunks(&level.level_folder, vec![(position, chunk.clone())])
        .await
        .unwrap();
    assert!(!chunk.is_dirty());
    assert!(level.schedule_block_tick_absolute(tick(1, 1_600, 8)));
    assert!(chunk.is_dirty());
    assert_eq!(level.prepare_chunk_poi(&chunk).unwrap(), stamp);
    // First flush completes the already-captured bytes and retains the dirty
    // exact source. Next flush consumes that receipt's recapture without submit.
    writer.flush().await.unwrap();
    assert!(chunk.is_dirty());
    writer.flush().await.unwrap();
    assert!(!chunk.is_dirty());
    let reader = ChunkFileManager::<AnvilChunkFile<ChunkData>>::new(AnvilChunkConfig::default());
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    reader
        .fetch_chunks(&level.level_folder, &[position], send)
        .await;
    let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
        panic!("missing recaptured chunk")
    };
    let saved_ticks = saved.block_ticks.pack_at(0);
    assert_eq!(saved_ticks.len(), 1);
    assert_eq!(
        (saved_ticks[0].position.0.x, saved_ticks[0].delay),
        (1, 600)
    );
    assert_eq!(level.prepare_chunk_poi(&chunk).unwrap(), stamp);
}
