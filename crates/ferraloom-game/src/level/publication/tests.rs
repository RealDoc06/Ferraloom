use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    level::{
        LevelContext, Region,
        acquisition::{
            AcquireDriver, AcquireFailure, AcquireHandle, AcquireProgress, AcquireResult,
        },
        heightmap::Heightmap,
        tickets::TicketStorage,
    },
    ticks::{LevelChunkTicks, SavedTick, TickPriority},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::Block;
use ferraloom_light::{DataLayer, LightLayer};
use ferraloom_nbt::Tag;
use ferraloom_util::{BlockPos, ChunkPos};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
};

const ORIGIN: ChunkPos = ChunkPos::new(0, 0);
const TARGET: ChunkPos = ChunkPos::new(1, 0);
const STONE: BlockPos = BlockPos::new(16, 0, 0);
const PORTAL: BlockPos = BlockPos::new(17, 0, 0);
static EMPTY: Behaviors = Behaviors::EMPTY;

fn chunk(pos: ChunkPos) -> LevelChunk {
    LevelChunk::new(
        pos,
        0..1,
        LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
            BiomeId(0),
        ),
    )
}
fn level() -> Level {
    use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.set_block_tags(Arc::new(TagSet::load(&resources).unwrap()));
    level.add_chunk(chunk(ORIGIN)).unwrap();
    level
}
fn prepared() -> PreparedFullChunk {
    let mut chunk = chunk(TARGET);
    chunk
        .set_block_state(STONE, Block::STONE.default_state())
        .unwrap();
    chunk
        .set_block_state(PORTAL, Block::END_PORTAL.default_state())
        .unwrap();
    chunk
        .add_prepared_block_entity(
            crate::block::entity::create(PORTAL, Block::END_PORTAL.default_state()).unwrap(),
        )
        .unwrap();
    chunk.block_ticks = LevelChunkTicks::from_saved(
        vec![SavedTick {
            pos: STONE,
            block: Block::SAND,
            delay: 7,
            priority: TickPriority::Normal,
        }],
        TARGET,
    );
    PreparedFullChunk {
        chunk,
        light: ChunkLightSnapshot {
            block: BTreeMap::from([(0, DataLayer::new(7))]),
            sky: BTreeMap::new(),
        },
        worldgen_entities: Vec::new(),
    }
}

struct PublishingDriver {
    prepared: RefCell<Option<PreparedFullChunk>>,
    published: Cell<Option<ChunkReference>>,
    tickets: RefCell<TicketStorage>,
    absent: bool,
}
impl PublishingDriver {
    fn new(prepared: Option<PreparedFullChunk>, absent: bool) -> Self {
        Self {
            prepared: RefCell::new(prepared),
            published: Cell::new(None),
            tickets: RefCell::new(TicketStorage::default()),
            absent,
        }
    }
}
impl AcquireDriver for PublishingDriver {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_owner_thread(&self) -> bool {
        true
    }
    fn visible_ticket_level(&self, pos: ChunkPos) -> Option<i32> {
        (pos == TARGET).then_some(33)
    }
    fn add_unknown_ticket(
        &self,
        pos: ChunkPos,
        status: super::super::chunk_holder::ChunkStatus,
    ) -> Result<(), LevelError> {
        self.tickets.borrow_mut().add_unknown(pos, status);
        Ok(())
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn request_status(
        &self,
        _: &Level,
        pos: ChunkPos,
        status: super::super::chunk_holder::ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        Ok(AcquireHandle {
            pos,
            status,
            request_id: 1,
            holder_epoch: 1,
        })
    }
    fn acquisition_result(&self, _: AcquireHandle) -> Result<AcquireResult, LevelError> {
        Ok(if self.absent {
            AcquireResult::Complete(None)
        } else if let Some(chunk) = self.published.get() {
            AcquireResult::Complete(Some(chunk))
        } else {
            AcquireResult::Pending
        })
    }
    fn poll_acquisition_step(&self, level: &Level) -> Result<AcquireProgress, LevelError> {
        let prepared = self.prepared.borrow_mut().take().unwrap();
        match level.stage_full_chunk(prepared) {
            Ok(staged) => {
                assert!(staged.worldgen_entities.is_empty());
                let reference = level.finalize_full_chunk(staged.token).unwrap();
                self.published.set(Some(reference));
                Ok(AcquireProgress::Task)
            }
            Err(error) => {
                self.prepared.replace(Some(error.prepared));
                Err(error.source)
            }
        }
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        panic!("publication completes in one managed step")
    }
    fn chunk_if_present(
        &self,
        pos: ChunkPos,
        _: super::super::chunk_holder::ChunkStatus,
    ) -> Option<ChunkReference> {
        self.published.get().filter(|chunk| chunk.pos == pos)
    }
    fn chunk_if_present_unchecked(
        &self,
        pos: ChunkPos,
        status: super::super::chunk_holder::ChunkStatus,
    ) -> Option<ChunkReference> {
        self.chunk_if_present(pos, status)
    }
    fn acquire_on_owner(
        &self,
        _: ChunkPos,
        _: super::super::chunk_holder::ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        panic!("owner-thread fixture")
    }
}

#[test]
fn managed_read_publishes_real_state_light_entities_and_pending_ticks_without_invalidating_borrows()
{
    let mut level = level();
    level.set_game_time(100);
    level
        .install_acquire_driver(
            Box::new(PublishingDriver::new(Some(prepared()), false)),
            0..16,
        )
        .unwrap();
    assert!(level.has_chunk(TARGET)); // eligible while generation is pending
    assert!(level.get_chunk_now(TARGET).unwrap().is_none());
    let old = level.chunk(ORIGIN).unwrap();
    assert_eq!(
        level.get_block_state(STONE).unwrap(),
        Block::STONE.default_state()
    );
    assert!(std::ptr::eq(old, level.chunk(ORIGIN).unwrap()));
    assert_eq!(level.brightness(LightLayer::Block, STONE).unwrap(), 7);
    let handle = level.block_entity_ref(PORTAL).unwrap().unwrap();
    assert_ne!(handle.generation, 0);
    assert!(level.block_entity_state(handle).unwrap().data().has_level);
    let ticks = level.region().block_ticks.container(TARGET).unwrap();
    assert!(ticks.has_scheduled_tick(STONE, Block::SAND));
    assert!(ticks.peek().is_none());
    assert_eq!(ticks.pack(1000)[0].delay, 7);
    level.set_game_time(500);
    level.start_ticking_chunk(TARGET).unwrap();
    assert_eq!(
        level
            .region()
            .block_ticks
            .container(TARGET)
            .unwrap()
            .peek()
            .unwrap()
            .trigger_tick,
        507
    );
    level.finish_chunk_source_tick();
}

#[test]
fn staged_entity_preflight_keeps_owned_nbt_and_does_not_complete_under_warn() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level();
        level.set_unported_behavior(policy);
        let mut prepared = prepared();
        let mut tag = CompoundTag::new();
        tag.insert("id", Tag::String("minecraft:pig".into()));
        tag.insert("retained", Tag::Double(-0.0));
        prepared.worldgen_entities.push(tag);
        let staged = level.stage_full_chunk(prepared).unwrap();
        let error = level
            .add_worldgen_chunk_entities_shared(TARGET, staged.worldgen_entities)
            .unwrap_err();
        assert_eq!(
            error.stage,
            crate::level::entity::WorldgenEntityLoadStage::Preflight
        );
        assert_eq!(
            error.entities[0].get_double("retained").unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert!(!level.chunk(TARGET).unwrap().is_loaded());
        assert!(level.get_chunk_now(TARGET).unwrap().is_none());
        assert_eq!(
            level.get_full_chunk(TARGET).err(),
            Some(LevelError::ChunkAcquisition(
                TARGET,
                super::super::chunk_holder::ChunkStatus::Full,
                AcquireFailure::ReentrantFull
            ))
        );
        assert!(level.region().block_ticks.container(TARGET).is_none());
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}

#[test]
fn required_acquisition_failures_propagate_in_both_policies_and_presence_does_not_load() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = level();
        level.set_unported_behavior(policy);
        level
            .install_acquire_driver(Box::new(PublishingDriver::new(None, true)), 0..16)
            .unwrap();
        assert_eq!(
            level.get_block_state(STONE),
            Err(LevelError::ChunkAcquisition(
                TARGET,
                super::super::chunk_holder::ChunkStatus::Full,
                AcquireFailure::MissingResult
            ))
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        let absent = ChunkPos::new(5, 0);
        assert!(!level.has_chunk(absent));
        assert!(level.get_chunk_for_collisions(absent).unwrap().is_none());
        assert_eq!(
            level.get_height(Heightmap::MotionBlocking, absent.x << 4, 0),
            Ok(0)
        );
        assert_eq!(
            level.get_height(Heightmap::MotionBlocking, 30_000_000, 0),
            Ok(64)
        );
    }
}

#[test]
fn duplicate_publication_keeps_the_canonical_receiver_and_returns_the_new_payload() {
    let level = level();
    let staged = level.stage_full_chunk(prepared()).unwrap();
    let reference = level.finalize_full_chunk(staged.token).unwrap();
    let original = level.chunk(TARGET).unwrap();
    let error = level.stage_full_chunk(prepared()).unwrap_err();
    assert_eq!(error.source, LevelError::DuplicateChunk(TARGET));
    assert!(std::ptr::eq(original, level.chunk(TARGET).unwrap()));
    assert_eq!(original.publication_generation, reference.generation);
    assert_eq!(error.prepared.chunk.pos(), TARGET);
}

#[test]
fn exclusive_attachment_registers_prepared_receivers_without_unpacking_saved_ticks() {
    let mut level = level();
    level.add_chunk(prepared().chunk).unwrap();
    let handle = level.block_entity_ref(PORTAL).unwrap().unwrap();
    assert_ne!(handle.generation, 0);
    assert!(level.block_entity_state(handle).unwrap().data().has_level);
    let ticks = level.region().block_ticks.container(TARGET).unwrap();
    assert!(ticks.has_scheduled_tick(STONE, Block::SAND));
    assert!(ticks.peek().is_none());
}

#[test]
fn driver_is_one_owned_runtime_and_cold_access_never_replaces_it() {
    let mut level = level();
    level
        .install_acquire_driver(
            Box::new(PublishingDriver::new(Some(prepared()), false)),
            0..16,
        )
        .unwrap();
    assert_eq!(
        level.install_acquire_driver(Box::new(PublishingDriver::new(None, true)), 0..16),
        Err(LevelError::ChunkAcquisition(
            ChunkPos::ZERO,
            super::super::chunk_holder::ChunkStatus::Full,
            AcquireFailure::DriverAlreadyInstalled,
        ))
    );
    let runtime = level
        .acquire_driver()
        .unwrap()
        .as_any()
        .downcast_ref::<PublishingDriver>()
        .unwrap();
    assert!(!runtime.absent);
    assert!(runtime.prepared.borrow().is_some());
    level
        .acquire_driver_mut()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<PublishingDriver>()
        .unwrap()
        .absent = true;
    assert!(
        level
            .acquire_driver()
            .unwrap()
            .as_any()
            .downcast_ref::<PublishingDriver>()
            .unwrap()
            .absent
    );
}

#[test]
fn installed_dimension_bounds_reject_a_wrong_first_chunk_without_changing_context() {
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level
        .install_acquire_driver(Box::new(PublishingDriver::new(None, true)), 0..16)
        .unwrap();
    let empty_section = chunk(ORIGIN).section(0).unwrap().clone();
    assert_eq!(
        level.add_chunk(LevelChunk::new(ORIGIN, -1..1, empty_section)),
        Err(LevelError::WrongChunkHeight(ORIGIN))
    );
    assert!(level.chunk(ORIGIN).is_none());
    assert_eq!(level.min_build_height(), 0);
    assert_eq!(level.max_build_height(), 15);
    assert_eq!(
        ferraloom_worldgen::feature::LiveWorldGenLevel::height(&level),
        16
    );
}

#[test]
fn staged_center_is_lower_status_only_until_post_load_and_finalize() {
    use crate::level::chunk_holder::ChunkStatus;
    let level = level();
    let staged = level.stage_full_chunk(prepared()).unwrap();
    let center = level.chunk(TARGET).unwrap();
    assert!(!center.is_loaded());
    assert_eq!(level.brightness(LightLayer::Block, STONE), Ok(7));
    assert_eq!(
        level.get_chunk(TARGET, ChunkStatus::Features, true),
        Ok(Some(staged.token.reference()))
    );
    for load in [false, true] {
        assert_eq!(
            level.get_chunk(TARGET, ChunkStatus::Full, load),
            Err(LevelError::ChunkAcquisition(
                TARGET,
                ChunkStatus::Full,
                AcquireFailure::ReentrantFull
            ))
        );
    }
    assert!(level.get_chunk_now(TARGET).unwrap().is_none());
    assert!(level.region().block_ticks.container(TARGET).is_none());
    let data = center.block_entities.get(PORTAL).unwrap().data();
    assert!(data.has_level && !data.removed && data.generation != 0);
    // Empty postLoad still belongs to the continuation, before finalization.
    level
        .add_worldgen_chunk_entities_shared(TARGET, staged.worldgen_entities)
        .unwrap();
    let reference = level.finalize_full_chunk(staged.token).unwrap();
    assert!(center.is_loaded());
    assert!(std::ptr::eq(center, level.get_full_chunk(TARGET).unwrap()));
    assert!(level.get_chunk_now(TARGET).unwrap().is_some());
    assert_eq!(
        level.finalize_full_chunk(staged.token),
        Err(ChunkFinalizationError {
            token: staged.token,
            checkpoint: FullChunkFinalizationStep::Finalized,
            failure: ChunkFinalizationFailure::AlreadyFinalized,
        })
    );
    assert_eq!(reference, staged.token.reference());
}

#[test]
fn finalization_retry_keeps_entities_ticker_binding_and_both_tick_queues() {
    use crate::entity::EntityData;
    let mut level = level();
    let mut input = prepared();
    let furnace = BlockPos::new(18, 0, 0);
    input
        .chunk
        .set_block_state(furnace, Block::FURNACE.default_state())
        .unwrap();
    input
        .chunk
        .add_prepared_block_entity(
            crate::block::entity::create(furnace, Block::FURNACE.default_state()).unwrap(),
        )
        .unwrap();
    input.chunk.fluid_ticks = LevelChunkTicks::from_saved(
        vec![SavedTick {
            pos: STONE,
            block: ferraloom_data::registries::fluid::Id::WATER,
            delay: 9,
            priority: TickPriority::Normal,
        }],
        TARGET,
    );
    input.worldgen_entities.push(
        ferraloom_nbt::parse_compound(
            "{id:\"minecraft:tnt\",UUID:[I;0,0,0,42],Pos:[16.0d,0.0d,0.0d]}",
        )
        .unwrap(),
    );
    let staged = level.stage_full_chunk(input).unwrap();
    level
        .update_entity_chunk_status(TARGET, crate::level::entity::Visibility::Tracked)
        .unwrap();
    level
        .add_worldgen_chunk_entities_shared(TARGET, staged.worldgen_entities)
        .unwrap();
    let entity = level
        .entity_by_uuid(ferraloom_util::Uuid::new(0, 42))
        .unwrap();
    let id = level.entity::<EntityData>(entity).unwrap().network_id;
    // A competing registration forces a later failure after BEs/block ticks.
    level
        .region_mut()
        .fluid_ticks
        .add_container(TARGET, LevelChunkTicks::default());
    let error = level.finalize_full_chunk(staged.token).unwrap_err();
    assert_eq!(
        error.checkpoint,
        FullChunkFinalizationStep::BlockTicksRegistered
    );
    assert_eq!(
        error.failure,
        ChunkFinalizationFailure::Registration(LevelError::DuplicateChunk(TARGET))
    );
    assert!(level.chunk(TARGET).unwrap().is_loaded());
    assert!(level.get_chunk_now(TARGET).unwrap().is_none());
    let blocks = level.region().block_ticks.container(TARGET).unwrap();
    assert_eq!(blocks.pack(1000)[0].delay, 7);
    let mut data = ferraloom_storage::SerializableChunkData::default();
    level.save_chunk_ticks(TARGET, &mut data).unwrap();
    assert_eq!(data.fluid_ticks.unwrap()[0].delay, Some(9));
    level.region_mut().fluid_ticks.remove_container(TARGET);
    let next_id = level.next_entity_id();
    level.finalize_full_chunk(error.token).unwrap();
    assert_eq!(level.next_entity_id(), next_id + 1);
    assert_eq!(level.entity::<EntityData>(entity).unwrap().network_id, id);
    assert_eq!(
        level
            .region()
            .fluid_ticks
            .container(TARGET)
            .unwrap()
            .pack(1000)[0]
            .delay,
        9
    );
    let mut tickers = level.region().block_entity_state.tickers.borrow_mut();
    tickers.begin_phase();
    assert_eq!(tickers.next_ticker().unwrap().entity.pos, furnace);
    assert!(tickers.next_ticker().is_none());
    tickers.end_phase();
}

#[test]
fn staged_unload_and_recreation_preserve_saved_ticks_and_reject_old_tokens() {
    let mut level = level();
    let staged = level.stage_full_chunk(prepared()).unwrap();
    let mut saved = ferraloom_storage::SerializableChunkData::default();
    level.save_chunk_ticks(TARGET, &mut saved).unwrap();
    assert_eq!(saved.block_ticks.as_ref().unwrap()[0].delay, Some(7));
    let chunk = level.remove_chunk(TARGET).unwrap();
    let mut unloaded = ferraloom_storage::SerializableChunkData::default();
    chunk.save_owned_ticks(999, &mut unloaded);
    assert_eq!(unloaded.block_ticks.unwrap()[0].delay, Some(7));
    let fresh = level
        .stage_full_chunk(PreparedFullChunk {
            chunk,
            light: ChunkLightSnapshot {
                block: BTreeMap::new(),
                sky: BTreeMap::new(),
            },
            worldgen_entities: Vec::new(),
        })
        .unwrap();
    assert_ne!(staged.token, fresh.token);
    assert_eq!(
        level.finalize_full_chunk(staged.token),
        Err(ChunkFinalizationError {
            token: staged.token,
            checkpoint: FullChunkFinalizationStep::Unavailable,
            failure: ChunkFinalizationFailure::StaleStage,
        })
    );
    level.finalize_full_chunk(fresh.token).unwrap();
}

#[test]
fn stage_identity_does_not_alias_another_level_with_the_same_generation() {
    let first = level();
    let second = level();
    let a = first.stage_full_chunk(prepared()).unwrap();
    let b = second.stage_full_chunk(prepared()).unwrap();
    assert_eq!(a.token.reference(), b.token.reference());
    assert_ne!(a.token, b.token);
    assert_eq!(
        second.finalize_full_chunk(a.token).unwrap_err().failure,
        ChunkFinalizationFailure::StaleStage
    );
    second.finalize_full_chunk(b.token).unwrap();
}
