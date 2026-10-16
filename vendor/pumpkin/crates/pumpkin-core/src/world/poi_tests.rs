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
