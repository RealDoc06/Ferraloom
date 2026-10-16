//! Stage C caller contracts, UNRUN pending root's native slot.

use super::*;
use crate::command::node::dispatcher::CommandDispatcher;
use pumpkin_data::{BlockStateId, entity::EntityType};
use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};
use pumpkin_util::permission::PermissionRegistry;
use pumpkin_world::chunk::ChunkData;
use std::sync::Arc;

#[tokio::test]
async fn locate_actual_raw_and_batch_writes_use_sphere_and_horizontal_result() {
    let fixture = crate::command::commands::data::tests::Fixture::new().await;
    let world = fixture.source.world();
    for chunk in &*world.level.loaded_chunks {
        world.level.bind_chunk_poi(chunk.value()).unwrap();
    }
    let chunk = world
        .level
        .loaded_chunks
        .get(&Vector2::new(0, 0))
        .unwrap()
        .clone();
    let origin = BlockPos::floored_v(fixture.source.position);
    // Same x/z: one exactly on the sphere, one just outside it. No World hook.
    chunk.set_blocks_batch([
        (0, origin.0.y + 256, 0, Block::LECTERN.default_state.id),
        (1, origin.0.y + 256, 0, Block::LECTERN.default_state.id),
    ]);
    let mut dispatcher = CommandDispatcher::new();
    crate::command::commands::locate::register(&mut dispatcher, &PermissionRegistry::default());
    assert_eq!(
        dispatcher
            .execute_input("locate poi minecraft:librarian", &fixture.source)
            .unwrap(),
        0
    );
    chunk.set_block_absolute_y(0, origin.0.y + 256, 0, BlockStateId::AIR);
    assert!(
        dispatcher
            .execute_input("locate poi minecraft:librarian", &fixture.source)
            .is_err()
    );
    chunk.set_block_absolute_y(3, origin.0.y, 4, Block::LECTERN.default_state.id);
    assert_eq!(
        dispatcher
            .execute_input("locate poi minecraft:librarian", &fixture.source)
            .unwrap(),
        5
    );
}

#[tokio::test]
async fn job_query_and_claim_keep_owner_identity_and_defer_missing_terrain() {
    let fixture = crate::command::commands::data::tests::Fixture::new().await;
    let world = fixture.source.world();
    let chunk = ChunkData::empty_sync(3, 0);
    world.level.bind_chunk_poi(&chunk).unwrap();
    for chunk in &*world.level.loaded_chunks {
        world.level.bind_chunk_poi(chunk.value()).unwrap();
    }
    world
        .level
        .loaded_chunks
        .insert(Vector2::new(3, 0), chunk.clone());
    chunk.set_blocks_batch([
        (0, 64, 0, Block::LECTERN.default_state.id),
        (1, 64, 0, Block::LECTERN.default_state.id),
    ]);
    let site = BlockPos::new(48, 64, 0);
    let entries = world.poi_in_sphere(BlockPos::new(0, 64, 0), 48).unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.poi_type == "minecraft:librarian")
            .map(PoiEntry::pos)
            .collect::<Vec<_>>(),
        vec![site]
    );
    let first = crate::entity::r#type::from_type(
        &EntityType::VILLAGER,
        Vector3::new(48.0, 64.0, 1.0),
        world,
        uuid::Uuid::from_u128(70),
    );
    let second = crate::entity::r#type::from_type(
        &EntityType::VILLAGER,
        Vector3::new(48.0, 64.0, 1.0),
        world,
        uuid::Uuid::from_u128(71),
    );
    assert!(matches!(
        world.claim_job_site(site, Arc::downgrade(&first), None),
        JobSiteClaim::Claimed
    ));
    assert!(matches!(
        world.claim_job_site(site, Arc::downgrade(&second), None),
        JobSiteClaim::Rejected
    ));
    world
        .villager_poi
        .lock()
        .unwrap()
        .release(site, second.get_entity().entity_uuid);
    assert!(matches!(
        world.claim_job_site(site, Arc::downgrade(&second), None),
        JobSiteClaim::Rejected
    ));
    world.level.loaded_chunks.remove(&Vector2::new(3, 0));
    assert!(matches!(
        world.claim_job_site(site, Arc::downgrade(&first), None),
        JobSiteClaim::Unavailable
    ));
    world.level.loaded_chunks.insert(Vector2::new(3, 0), chunk);
    first.get_living_entity().unwrap().health.store(0.0);
    assert_eq!(
        world.available_job_sites(site, Some(VillagerProfession::Librarian)),
        vec![site]
    );
    assert!(matches!(
        world.claim_job_site(site, Arc::downgrade(&second), None),
        JobSiteClaim::Claimed
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_actual_palette_writes_use_empty_area_and_only_candidate_full_columns() {
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::block_properties::{HorizontalAxis, NetherPortalLikeProperties};
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::{level::Level, world_info::LevelData};
    use std::time::Duration;
    let directory = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        directory.path().into(),
        0,
        Dimension::OVERWORLD,
    );
    let chunks = [ChunkData::empty_sync(0, 0), ChunkData::empty_sync(1, 0)];
    for chunk in &chunks {
        level.bind_chunk_poi(chunk).unwrap();
        level
            .loaded_chunks
            .insert(Vector2::new(chunk.x, chunk.z), chunk.clone());
    }
    let state = NetherPortalLikeProperties {
        axis: HorizontalAxis::X,
    }
    .to_state_id(&Block::NETHER_PORTAL);
    // A real frame crosses the chunk edge. Neither World publication hook runs.
    for x in 14..=17 {
        let chunk = &chunks[(x >> 4) as usize];
        chunk.set_blocks_batch((63..=67).map(|y| {
            let block = if x == 14 || x == 17 || y == 63 || y == 67 {
                Block::OBSIDIAN.default_state.id
            } else {
                state
            };
            ((x & 15) as usize, y, 1, block)
        }));
    }
    chunks[0].set_block_absolute_y(15, 64, 1, BlockStateId::AIR);
    chunks[0].set_block_absolute_y(15, 64, 1, state);
    let world = Arc::new(World::load(
        level,
        Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
        Dimension::OVERWORLD,
        crate::block::registry::default_registry(),
        std::sync::Weak::new(),
        Arc::default(),
    ));
    let query_world = world.clone();
    let found = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            portal::nether::NetherPortal::search_for_portal(&query_world, BlockPos::new(15, 64, 1))
        }),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(found.lower_corner, BlockPos::new(15, 64, 1));
    assert_eq!((found.width, found.height), (2, 3));
    assert_eq!(found.axis, HorizontalAxis::X);
    assert_eq!(world.level.loaded_chunks.len(), 2);
    assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
    for x in 15..=16 {
        chunks[(x >> 4) as usize]
            .set_blocks_batch((64..=66).map(|y| ((x & 15) as usize, y, 1, BlockStateId::AIR)));
    }
    let query_world = world.clone();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(10),
            tokio::task::spawn_blocking(move || {
                portal::nether::NetherPortal::search_for_portal(
                    &query_world,
                    BlockPos::new(15, 64, 1),
                )
            })
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .is_none()
    );
    assert_eq!(world.level.loaded_chunks.len(), 2);
    tokio::time::timeout(Duration::from_secs(10), world.level.shutdown())
        .await
        .unwrap()
        .unwrap();
}

// Destination rectangle contracts below are UNRUN pending root's native slot.
fn rectangle_fixture(columns: &[(i32, i32)]) -> (tempfile::TempDir, Arc<World>) {
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::{level::Level, world_info::LevelData};
    let directory = tempfile::tempdir().unwrap();
    let level = Level::from_root_folder(
        &LevelConfig::default(),
        directory.path().into(),
        0,
        Dimension::THE_NETHER,
    );
    for &(x, z) in columns {
        let chunk = ChunkData::empty_sync(x, z);
        level.bind_chunk_poi(&chunk).unwrap();
        level.loaded_chunks.insert(Vector2::new(x, z), chunk);
    }
    let world = Arc::new(World::load(
        level,
        Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
        Dimension::THE_NETHER,
        crate::block::registry::default_registry(),
        std::sync::Weak::new(),
        Arc::default(),
    ));
    (directory, world)
}

fn rectangle_state(axis: pumpkin_data::block_properties::HorizontalAxis) -> BlockStateId {
    pumpkin_data::block_properties::NetherPortalLikeProperties { axis }
        .to_state_id(&Block::NETHER_PORTAL)
}

async fn rectangle_search(
    world: &Arc<World>,
    target: BlockPos,
) -> std::io::Result<Option<portal::nether::PortalSearchResult>> {
    let query_world = world.clone();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            portal::nether::NetherPortal::search_for_portal(&query_world, target)
        }),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn rectangle_shutdown(world: &World) -> Result<(), String> {
    tokio::time::timeout(std::time::Duration::from_secs(10), world.level.shutdown())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_rectangle_prefers_unframed_single_state_and_record_y_tie() {
    use pumpkin_data::block_properties::HorizontalAxis;
    let (_directory, world) = rectangle_fixture(&[(0, 0)]);
    let chunk = world
        .level
        .loaded_chunks
        .get(&Vector2::new(0, 0))
        .unwrap()
        .clone();
    let state = rectangle_state(HorizontalAxis::X);
    let target = BlockPos::new(4, 64, 4);
    chunk.set_block_absolute_y(4, 64, 4, state);
    chunk.set_blocks_batch((10..=13).flat_map(|x| {
        (63..=67).map(move |y| {
            let block = if x == 10 || x == 13 || y == 63 || y == 67 {
                Block::OBSIDIAN.default_state.id
            } else {
                state
            };
            (x, y, 4, block)
        })
    }));
    let found = rectangle_search(&world, target).await.unwrap().unwrap();
    assert_eq!(found.lower_corner, target);
    assert_eq!((found.width, found.height), (1, 1));
    assert_eq!(found.axis, HorizontalAxis::X);
    assert_eq!(world.get_block_state_id(&target), state);
    assert_eq!(world.get_block(&BlockPos::new(10, 64, 4)), &Block::OBSIDIAN);
    assert_eq!(world.get_block_state_id(&BlockPos::new(11, 64, 4)), state);

    // Equidistant records: choose record Y before building either rectangle.
    chunk.set_block_absolute_y(4, 64, 4, BlockStateId::AIR);
    chunk.set_blocks_batch([(4, 62, 4, state), (4, 66, 4, state)]);
    let found = rectangle_search(&world, target).await.unwrap().unwrap();
    assert_eq!(found.lower_corner, BlockPos::new(4, 62, 4));
    assert_eq!((found.width, found.height), (1, 1));
    assert_eq!(world.level.loaded_chunks.len(), 1);
    assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
    rectangle_shutdown(&world).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_rectangle_exact_state_irregular_histogram_and_bottom_ties() {
    use pumpkin_data::block_properties::HorizontalAxis;
    let (_directory, world) = rectangle_fixture(&[(0, 0)]);
    let chunk = world
        .level
        .loaded_chunks
        .get(&Vector2::new(0, 0))
        .unwrap()
        .clone();
    let state = rectangle_state(HorizontalAxis::X);
    let other_axis = rectangle_state(HorizontalAxis::Z);
    let center = BlockPos::new(8, 64, 8);
    // Histogram [1,3,1]: the center 1x3 is popped before the equal-area 3x1.
    // Same portal block, different state at both horizontal/vertical limits.
    chunk.set_blocks_batch([
        (6, 64, 8, other_axis),
        (7, 64, 8, state),
        (8, 64, 8, state),
        (9, 64, 8, state),
        (8, 65, 8, state),
        (8, 66, 8, state),
        (8, 67, 8, other_axis),
    ]);
    let found = rectangle_search(&world, center).await.unwrap().unwrap();
    assert_eq!(found.lower_corner, center);
    assert_eq!((found.width, found.height), (1, 3));
    assert_eq!(found.axis, HorizontalAxis::X);
    assert_eq!(
        world.get_block_state_id(&BlockPos::new(8, 67, 8)),
        other_axis
    );

    // At bottom Y=64 the histogram gives 3x2. At Y=63 it gives 2x3;
    // equal area must retain the first, higher bottom, not the lower corner.
    chunk.set_block_absolute_y(8, 66, 8, BlockStateId::AIR);
    chunk.set_blocks_batch([
        (7, 65, 8, state),
        (9, 65, 8, state),
        (7, 63, 8, state),
        (8, 63, 8, state),
    ]);
    let found = rectangle_search(&world, center).await.unwrap().unwrap();
    assert_eq!(found.lower_corner, BlockPos::new(7, 64, 8));
    assert_eq!((found.width, found.height), (3, 2));
    assert_eq!(world.level.loaded_chunks.len(), 1);
    assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
    rectangle_shutdown(&world).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_rectangle_caps_each_direction_at_twenty_one_without_halo() {
    use pumpkin_data::block_properties::HorizontalAxis;
    let (_directory, world) = rectangle_fixture(&[(0, 0), (0, 1), (0, 2), (0, 3)]);
    let state = rectangle_state(HorizontalAxis::Z);
    // A Z-aligned 45x45 plane extends past every scan cap. No frame exists.
    for column in 0..=3 {
        let chunk = world
            .level
            .loaded_chunks
            .get(&Vector2::new(0, column))
            .unwrap()
            .clone();
        chunk.set_blocks_batch(
            (0..16)
                .filter_map(|z| {
                    let absolute_z = column * 16 + z;
                    (10..=54).contains(&absolute_z).then_some(z as usize)
                })
                .flat_map(|z| (42..=86).map(move |y| (8, y, z, state))),
        );
    }
    let found = rectangle_search(&world, BlockPos::new(8, 64, 32))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.lower_corner, BlockPos::new(8, 43, 11));
    assert_eq!((found.width, found.height), (43, 43));
    assert_eq!(found.axis, HorizontalAxis::Z);
    assert_eq!(world.get_block_state_id(&BlockPos::new(8, 42, 10)), state);
    assert_eq!(world.get_block_state_id(&BlockPos::new(8, 86, 54)), state);
    assert_eq!(world.level.loaded_chunks.len(), 4);
    assert!(
        world
            .level
            .loaded_chunks
            .iter()
            .all(|entry| entry.key().x == 0)
    );
    assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
    rectangle_shutdown(&world).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn portal_rectangle_visited_column_refusal_aborts_before_farther_portal() {
    use pumpkin_data::block_properties::HorizontalAxis;
    use pumpkin_world::level::ChunkUnavailable;
    let (_directory, world) = rectangle_fixture(&[(30, 0), (31, 0)]);
    let state = rectangle_state(HorizontalAxis::X);
    let target = BlockPos::new(495, 64, 8);
    let selected = BlockPos::new(511, 64, 8);
    let chunk = world
        .level
        .loaded_chunks
        .get(&Vector2::new(31, 0))
        .unwrap()
        .clone();
    chunk.set_block_absolute_y(15, 64, 8, state);
    let farther = world
        .level
        .loaded_chunks
        .get(&Vector2::new(30, 0))
        .unwrap()
        .clone();
    farther.set_blocks_batch((0..=3).flat_map(|x| {
        (95..=99).map(move |y| {
            let block = if x == 0 || x == 3 || y == 95 || y == 99 {
                Block::OBSIDIAN.default_state.id
            } else {
                state
            };
            (x, y, 8, block)
        })
    }));
    // EMPTY square ends at X=511. Only the chosen rectangle's positive-axis
    // probe reaches X=512 and this refused terrain region. No fake read hook.
    let path = world.level.level_folder.region_folder.join("r.1.0.mca");
    let original = b"refused actual rectangle probe region";
    std::fs::write(&path, original).unwrap();
    let entries = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        world.level.portal_poi_snapshot(target, 16),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(entries.iter().any(|entry| entry.pos() == selected));
    assert!(
        entries
            .iter()
            .any(|entry| entry.pos() == BlockPos::new(481, 96, 8))
    );
    assert!(
        world
            .level
            .chunk_listener
            .failure(Vector2::new(32, 0))
            .is_none()
    );

    let error = rectangle_search(&world, target).await.unwrap_err();
    assert!(matches!(
        error
            .get_ref()
            .and_then(|error| error.downcast_ref::<ChunkUnavailable>()),
        Some(ChunkUnavailable::ReadFailed { .. })
    ));
    assert!(
        world
            .level
            .chunk_listener
            .failure(Vector2::new(32, 0))
            .is_some()
    );
    // The public transition must also abort; it may not create or fall back.
    let query_world = world.clone();
    let transition_absent = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::task::spawn_blocking(move || {
            let caller = crate::entity::Entity::new(
                query_world.clone(),
                target.0.to_f64(),
                &EntityType::PIG,
            );
            portal::PortalType::Nether
                .get_portal_destination(&query_world, query_world.clone(), &caller, None)
                .is_none()
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(transition_absent);
    assert_eq!(world.get_block_state_id(&selected), state);
    assert_eq!(world.get_block_state_id(&BlockPos::new(481, 96, 8)), state);
    assert_eq!(world.level.loaded_chunks.len(), 2);
    assert!(!world.level.is_chunk_loaded(&Vector2::new(32, 0)));
    assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
    assert!(rectangle_shutdown(&world).await.is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}
