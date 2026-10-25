use super::*;
use crate::world::WorldRoot;
use ferraloom_game::{
    Behaviors,
    level::{LevelContext, Region, acquisition::ChunkKind},
};
use std::time::Duration;

struct InlineProbe {
    owner: PhaseOwner,
    forwarded: Cell<u32>,
}
impl AcquireDriver for InlineProbe {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        self.owner.is_current()
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        Some(36)
    }
    fn add_unknown_ticket(&self, _: ChunkPos, _: ChunkStatus) -> Result<(), LevelError> {
        Ok(())
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn request_status(
        &self,
        _: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        Ok(AcquireHandle {
            pos,
            status,
            request_id: 1,
            holder_epoch: 1,
        })
    }
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        Ok(AcquireResult::Complete(Some(ChunkReference {
            pos: handle.pos,
            generation: 1,
            kind: ChunkKind::Proto,
        })))
    }
    fn poll_acquisition_step(&self, _: &Level) -> Result<AcquireProgress, LevelError> {
        panic!("completed acquisition must return inline")
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        panic!("completed acquisition must not wait")
    }
    fn chunk_if_present(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn acquire_on_owner(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        self.forwarded.set(self.forwarded.get() + 1);
        Err(LevelError::ChunkAcquisition(
            pos,
            status,
            AcquireFailure::DriverUnavailable,
        ))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acquisition_stays_inline_when_the_phase_task_moves_between_workers() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let (blocked, wait_blocked) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let owner = PhaseOwner::current();
        tokio::task::yield_now().await;
        let before = thread::current().id();
        let (resume, wait_resume) = tokio::sync::oneshot::channel();
        let (release, wait_release) = mpsc::channel();
        // A locally spawned LIFO task occupies this executor worker while the
        // server is suspended. Waking the server from outside that worker makes
        // its continuation run on the other worker, with the same Tokio task ID.
        let blocker = tokio::spawn(async move {
            let worker = thread::current().id();
            let _ = blocked.send((before, worker, resume, release));
            wait_release.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        wait_resume.await.unwrap();
        let after = thread::current().id();
        level
            .install_acquire_driver(
                Box::new(InlineProbe {
                    owner,
                    forwarded: Cell::new(0),
                }),
                0..16,
            )
            .unwrap();
        let acquired = level.get_chunk(ChunkPos::ZERO, ChunkStatus::Biomes, true);
        let forwarded = level
            .acquire_driver()
            .unwrap()
            .as_any()
            .downcast_ref::<InlineProbe>()
            .unwrap()
            .forwarded
            .get();
        (after, acquired, forwarded, blocker)
    });
    let (before, blocked_worker, resume, release) =
        tokio::time::timeout(Duration::from_secs(10), wait_blocked)
            .await
            .unwrap()
            .unwrap();
    resume.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), server).await;
    // Release the occupied worker before asserting, including on a failed test.
    let _ = release.send(());
    let (after, acquired, forwarded, blocker) = result.unwrap().unwrap();
    blocker.await.unwrap();
    assert_eq!(before, blocked_worker);
    assert_ne!(before, after);
    assert_eq!(forwarded, 0);
    assert_eq!(acquired.unwrap().unwrap().kind, ChunkKind::Proto);
}

#[tokio::test]
async fn resumed_full_promotion_keeps_the_successful_entity_post_load_checkpoint() {
    use crate::{registries::RegistrySet, world::World};
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use ferraloom_game::level::{
        publication::{
            ChunkFinalizationError, ChunkFinalizationFailure, FullChunkFinalizationStep,
        },
        server_level::ServerLevel,
    };
    use std::path::Path;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registries = RegistrySet::load(&resources).unwrap();
    let mut properties = ServerProperties::from_values(Default::default());
    properties.set("level-seed", "0");
    properties.set("level-type", "minecraft:debug_all_block_states");
    let directory = tempfile::tempdir().unwrap();
    let mut world_root = WorldRoot::open(
        directory.path(),
        &properties,
        &resources,
        &registries,
        ferraloom_game::block::UnportedBehavior::Strict,
    )
    .unwrap();
    let mut world = world_root
        .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
        .unwrap();
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut bootstrap = ServerLevel::new(
        Region::new(&BEHAVIORS),
        "minecraft:overworld",
        world.level_context(&resources).unwrap(),
    );
    bootstrap
        .level
        .set_block_tags(world.generator().access().data().block_tags.clone());
    let pos = ChunkPos::new(64, 0);
    bootstrap.chunks.ensure_holder(pos).ticket_level = 33;
    bootstrap.chunks.update_highest_allowed_status(pos);
    let task = bootstrap
        .chunks
        .schedule_generation_task(pos, ChunkStatus::Full)
        .task
        .schedule
        .unwrap()
        .id;
    world.request(task, pos);
    let (_, mut terrain) = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            world.poll_generation_progress().unwrap();
            if let Some(terrain) = world.take_generation_completion().unwrap() {
                break terrain;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    terrain.disk.entities = Some(vec![ferraloom_nbt::parse_compound(
        "{id:\"minecraft:tnt\",UUID:[I;1,2,3,4],Pos:[1024.5d,0.0d,0.5d],Motion:[0d,0d,0d],Rotation:[0f,0f]}",
    ).unwrap()]);
    let mut live = LiveLevel::install(world, bootstrap, Arc::new(properties)).unwrap();
    let runtime = live.generation();
    runtime
        .accept_test_terrain(&live.level, task, terrain)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(light) = runtime.take_light_completion().unwrap() {
                runtime.import_light(&live.level, light).unwrap();
            }
            if runtime.with_world(|world| world.full_promotions[&task].center_light_installed) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    runtime.jobs.borrow_mut().begin_integration(task);
    runtime.stage_full_promotion(&live.level, task).unwrap();
    let token = runtime.with_world(|world| world.full_promotions[&task].stage.unwrap());
    assert_eq!(
        live.level
            .get_chunk(pos, ChunkStatus::Biomes, false)
            .unwrap(),
        Some(token.reference())
    );
    assert!(live.level.get_chunk_now(pos).unwrap().is_none());
    runtime.with_world_mut(|world| {
        world
            .full_promotions
            .get_mut(&task)
            .unwrap()
            .post_load_active = true
    });
    assert_eq!(
        live.level.get_chunk(pos, ChunkStatus::Full, false),
        Err(LevelError::ChunkAcquisition(
            pos,
            ChunkStatus::Full,
            AcquireFailure::ReentrantFull
        ))
    );
    runtime.with_world_mut(|world| {
        world
            .full_promotions
            .get_mut(&task)
            .unwrap()
            .post_load_active = false
    });
    GenerationRuntime::run_post_load_with_access(
        &mut GenerationAccess::Mutable(&mut live.level),
        task,
    )
    .unwrap();
    let runtime = live.generation();
    let allocated_probe = live.level.next_entity_id();
    assert_eq!(allocated_probe, 2);
    // A cold executor failure after postLoad precedes the next finalize action.
    // Level's own test exercises the actual private tick-registration conflict;
    // this boundary verifies the runtime's permanent entity success checkpoint.
    runtime.with_world_mut(|world| {
        world
            .full_promotions
            .get_mut(&task)
            .unwrap()
            .finalization_failure = Some(ChunkFinalizationError {
            token,
            checkpoint: FullChunkFinalizationStep::Staged,
            failure: ChunkFinalizationFailure::Registration(LevelError::DuplicateChunk(pos)),
        })
    });
    runtime.jobs.borrow_mut().suspend_integration(task);
    GenerationRuntime::resume_full_promotion_with_access(
        &mut GenerationAccess::Mutable(&mut live.level),
        task,
    )
    .unwrap();
    let runtime = live.generation();
    assert_eq!(live.level.next_entity_id(), allocated_probe + 1);
    assert_eq!(live.level.take_entity_management_events().len(), 1);
    assert!(runtime.with_world(|world| world.full_promotions[&task].entities_registered));
    assert!(runtime.with_world(|world| world.full_promotions[&task].finalized));
    assert!(runtime.with_world(|world| world.light_ready.contains(&pos)));
    assert!(!runtime.with_world(|world| world.ready.contains(&pos)));
    assert_entity_visibility_follows_holder_ranges(&mut live.level, pos);
    assert_eq!(live.level.next_entity_id(), allocated_probe + 2);
    let runtime = live.generation();
    // A later task can demote the holder before its queued range confirmation
    // runs. Validate the cancellable token when publishing visibility.
    let (epoch, promotion) = runtime.with_chunks_mut(|chunks| {
        let holder = chunks.holders.get_mut(&pos.pack()).unwrap();
        holder.ticket_level = 31;
        holder.update_full_status();
        (holder.epoch, holder.pending_full_status().unwrap())
    });
    runtime
        .status_publications
        .borrow_mut()
        .push_back(StatusPublication {
            pos,
            epoch,
            confirmed: None,
            promotion: Some(promotion),
        });
    runtime.with_chunks_mut(|chunks| {
        let holder = chunks.holders.get_mut(&pos.pack()).unwrap();
        holder.ticket_level = 45;
        holder.update_full_status();
    });
    runtime.publish_status_changes(&live.level).unwrap();
    assert_eq!(
        live.level.entity_chunk_visibility(pos),
        ferraloom_game::level::entity::Visibility::Hidden
    );
    let drain = runtime
        .with_world_mut(World::prepare_close_chunk_storage)
        .unwrap();
    for task in drain.proto {
        task.await.unwrap();
    }
    for task in drain.chunks {
        task.await.unwrap().unwrap();
    }
    let close = runtime
        .with_world_mut(World::begin_close_chunk_storage)
        .unwrap();
    close.await.unwrap();
}

/// Controlled neighboring status futures isolate the native range requirements;
/// the center, driver, light publication and postLoad above use the real path.
fn assert_entity_visibility_follows_holder_ranges(level: &mut Level, pos: ChunkPos) {
    use ferraloom_game::level::entity::{EntityManagementEvent, Visibility};
    GenerationRuntime::from_level(level).with_chunks_mut(|chunks| {
        for x in pos.x - 2..=pos.x + 2 {
            for z in pos.z - 2..=pos.z + 2 {
                let neighbor = ChunkPos::new(x, z);
                if neighbor != pos {
                    chunks.ensure_holder(neighbor).generation = Default::default();
                }
            }
        }
    });
    let set_ticket = |level: &mut Level, ticket_level| {
        let runtime = GenerationRuntime::from_level(level);
        let publication = runtime.with_chunks_mut(|chunks| {
            let holder = chunks.holders.get_mut(&pos.pack()).unwrap();
            holder.ticket_level = ticket_level;
            StatusPublication {
                pos,
                epoch: holder.epoch,
                confirmed: holder.update_full_status(),
                promotion: None,
            }
        });
        runtime
            .status_publications
            .borrow_mut()
            .push_back(publication);
        GenerationRuntime::publish_status_changes_with_access(&mut GenerationAccess::Mutable(
            level,
        ))
        .unwrap();
    };
    let complete_neighbors = |level: &Level, radius, target: ChunkStatus| {
        GenerationRuntime::from_level(level).with_chunks_mut(|chunks| {
            for x in pos.x - radius..=pos.x + radius {
                for z in pos.z - radius..=pos.z + radius {
                    let neighbor = ChunkPos::new(x, z);
                    if neighbor == pos {
                        continue;
                    }
                    let holder = chunks.ensure_holder(neighbor);
                    let reference = ChunkReference {
                        pos: neighbor,
                        generation: holder.epoch,
                        kind: if target == ChunkStatus::Full {
                            ChunkKind::Full
                        } else {
                            ChunkKind::Proto
                        },
                    };
                    for status in STATUSES[..=target as usize].iter().copied() {
                        if holder.generation.acquire_status_bump(status) {
                            holder.generation.complete(status, reference);
                        }
                    }
                }
            }
        });
    };
    set_ticket(level, 45);
    set_ticket(level, 33);
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
    complete_neighbors(level, 1, ChunkStatus::InitializeLight);
    // A genuinely shared read still fails before the tracking callback. Use a
    // fresh native promotion after demotion for the independent mutable path.
    assert_eq!(
        GenerationRuntime::from_level(level).confirm_ready_ranges(level),
        Err(LevelError::UnsupportedSharedEntityTracking(
            "update_entity_chunk_status_shared"
        )),
    );
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
    set_ticket(level, 45);
    set_ticket(level, 33);
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Tracked);
    set_ticket(level, 32);
    complete_neighbors(level, 1, ChunkStatus::Full);
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert!(!level.chunk(pos).unwrap().ticking_ready());
    // Controlled ticking future; its actual postProcess/startTicking callbacks
    // are covered by the Level and live join tests, independently of this range.
    GenerationRuntime::from_level(level).with_world_mut(|world| {
        world.ready.insert(pos);
    });
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert!(level.chunk(pos).unwrap().ticking_ready());
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Tracked);
    set_ticket(level, 31);
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Tracked);
    complete_neighbors(level, 2, ChunkStatus::Full);
    GenerationRuntime::confirm_ready_ranges_mut(level).unwrap();
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Ticking);
    set_ticket(level, 33);
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Tracked);
    set_ticket(level, 45);
    assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
    let events = level.take_entity_management_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, EntityManagementEvent::TrackingStart(_)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, EntityManagementEvent::TrackingEnd(_)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, EntityManagementEvent::TickingStart(_)))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, EntityManagementEvent::TickingEnd(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn structure_status_reads_settle_before_a_queued_full_tracking_failure() {
    use crate::registries::RegistrySet;
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use ferraloom_game::level::{
        entity::{Visibility, WorldgenEntityLoadStage},
        server_level::ServerLevel,
    };
    use ferraloom_storage::SerializableChunkData;
    use std::path::Path;

    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let registries = RegistrySet::load(&resources).unwrap();
    let mut properties = ServerProperties::from_values(Default::default());
    properties.set("level-seed", "0");
    properties.set("level-type", "minecraft:debug_all_block_states");
    let directory = tempfile::tempdir().unwrap();
    let mut root = WorldRoot::open(
        directory.path(),
        &properties,
        &resources,
        &registries,
        ferraloom_game::block::UnportedBehavior::Strict,
    )
    .unwrap();
    let world = root
        .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
        .unwrap();
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut bootstrap = ServerLevel::new(
        Region::new(&BEHAVIORS),
        "minecraft:overworld",
        world.level_context(&resources).unwrap(),
    );
    bootstrap
        .level
        .set_block_tags(world.generator().access().data().block_tags.clone());
    let mut live = LiveLevel::install(world, bootstrap, Arc::new(properties)).unwrap();

    // Fresh terrain is acquired at references, without ever asking for FULL.
    let fresh = ChunkPos::new(64, 0);
    let empty = live
        .level
        .chunk_structures(fresh, ChunkStatus::StructureReferences)
        .unwrap();
    assert_eq!(empty.position, [64, 0]);
    assert!(empty.references.is_empty());
    assert!(live.level.chunk(fresh).is_none());
    let reference = live
        .level
        .get_chunk(fresh, ChunkStatus::StructureReferences, false)
        .unwrap()
        .unwrap();
    assert_eq!(reference.kind, ChunkKind::Proto);
    assert!(
        live.generation()
            .chunk_if_present_unchecked(fresh, ChunkStatus::Full)
            .is_none()
    );
    // A removed/recreated holder cannot read its predecessor's retained data.
    let mut stale = reference;
    stale.generation += 1;
    assert!(matches!(
        live.generation().proto_chunk_structures(stale),
        Err(LevelError::ChunkAcquisition(
            _,
            _,
            AcquireFailure::InvalidHandle
        ))
    ));

    let pos = ChunkPos::new(96, 0);
    let entity = ferraloom_nbt::parse_compound(
        "{id:\"minecraft:tnt\",UUID:[I;1,2,3,4],Pos:[1536.5d,80.0d,0.5d],Motion:[0d,0d,0d],Rotation:[0f,0f]}"
    ).unwrap();
    let disk = SerializableChunkData {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        x_pos: Some(pos.x),
        z_pos: Some(pos.z),
        y_pos: Some(-4),
        status: Some("minecraft:full".into()),
        entities: Some(vec![entity]),
        ..Default::default()
    };
    let saved = live
        .generation()
        .with_world(|world| world.io.store(pos, Some(disk.write().unwrap())));
    saved.await.unwrap();
    live.generation()
        .with_tickets_mut(|tickets| tickets.add_unknown(pos, ChunkStatus::Full));
    live.update_chunk_tickets().unwrap();
    // No entity exists yet, so changing visibility itself requires no callback.
    live.level
        .update_entity_chunk_status(pos, Visibility::Tracked)
        .unwrap();
    let full = live
        .generation()
        .request_status(&live.level, pos, ChunkStatus::Full)
        .unwrap();
    let structures = live
        .level
        .chunk_structures(pos, ChunkStatus::StructureReferences)
        .unwrap();
    assert_eq!(structures.position, [96, 0]);
    assert!(live.level.chunk(pos).is_none());
    assert_eq!(
        live.generation().acquisition_result(full).unwrap(),
        AcquireResult::Pending
    );
    let task = live.generation().with_chunks(|chunks| {
        chunks.holders[&pos.pack()]
            .generation
            .current_task()
            .unwrap()
            .id
    });
    assert!(
        !live
            .generation()
            .with_world(|world| world.full_promotions[&task].entities_registered)
    );
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(light) = live.generation().take_light_completion().unwrap() {
                live.generation().import_light(&live.level, light).unwrap();
            }
            if live
                .generation()
                .with_world(|world| world.full_promotions[&task].center_light_installed)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        GenerationRuntime::resume_full_promotion_with_access(
            &mut GenerationAccess::Shared {
                runtime: live.generation(),
                level: &live.level
            },
            task
        )
        .is_err()
    );
    live.generation().with_world(|world| {
        let error = world.full_promotions[&task]
            .entity_failure
            .as_ref()
            .unwrap();
        assert_eq!(error.stage, WorldgenEntityLoadStage::Preflight);
        assert!(matches!(
            error.source,
            ferraloom_game::entity::persistence::EntityPersistenceError::Level(
                LevelError::UnsupportedSharedEntityTracking(_)
            )
        ));
    });
    // The lower future and its source remain usable after that separate error.
    let retained = live
        .level
        .chunk_structures(pos, ChunkStatus::StructureReferences)
        .unwrap();
    assert!(Arc::ptr_eq(&structures, &retained));
    assert_eq!(empty.position, [64, 0]);
    let drain = live
        .generation()
        .with_world_mut(World::prepare_close_chunk_storage)
        .unwrap();
    for saving in drain.proto {
        saving.await.unwrap();
    }
    for saving in drain.chunks {
        saving.await.unwrap().unwrap();
    }
    let closing = live
        .generation()
        .with_world_mut(World::begin_close_chunk_storage)
        .unwrap();
    closing.await.unwrap();
}
