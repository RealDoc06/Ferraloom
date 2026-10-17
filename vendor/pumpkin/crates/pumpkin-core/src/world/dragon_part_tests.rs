//! Authored UNRUN: real entity persistence/unload, clipping and owner lookup.

use super::*;
use crate::{
    data::player_server::test::{close_fixture, player_fixture, stop_chunk_workers},
    entity::{
        RemovalReason,
        boss::ender_dragon::{EnderDragonEntity, phase::EnderDragonPhase},
    },
};
use pumpkin_data::damage::DamageType;
use pumpkin_nbt::tag::NbtTag;

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep the persisted unload, replacement activation and retained-part checks in one lifecycle fixture"
)]
async fn real_chunk_snapshot_and_unload_save_parent_only_and_retained_parts_stay_inert() {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let world = fixture.player.world();
    let position = Vector2::new(0, 0);
    let old = from_type(
        &EntityType::ENDER_DRAGON,
        Vector3::new(8.0, 80.0, 8.0),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(old.clone()));
    let d = old.cast_any().downcast_ref::<EnderDragonEntity>().unwrap();
    d.set_phase(EnderDragonPhase::SitScanning);
    let retained = d.parts()[0].clone();
    let chunk = world.level.get_entity_chunk(position).await.unwrap();
    chunk.live.store(true, Relaxed);
    let residents = world.entities.load_full();
    world
        .save_entities_by_chunk(&residents, [position])
        .await
        .unwrap();
    drop(residents);
    {
        let saved = chunk.data.lock().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(
            saved[0].get_uuid("UUID"),
            Some(old.get_entity().entity_uuid)
        );
        assert_eq!(saved[0].get_int("DragonPhase"), Some(6));
        assert!(
            saved[0]
                .get_list("Passengers")
                .is_none_or(<[NbtTag]>::is_empty)
        );
    };
    assert!(
        world
            .remove_entities_in_chunks([position])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !old.get_entity().is_removed(),
        "unload removes residency without a normal remove flag"
    );
    assert!(world.get_entity_by_id(old.get_entity().entity_id).is_none());
    assert!(
        world
            .get_entity_or_part(retained.entity.entity_id)
            .is_none()
    );
    assert!(retained.current_parent().is_none());
    assert!(!retained.damage_with_context(
        retained.as_ref(),
        8.0,
        DamageType::PLAYER_ATTACK,
        None,
        Some(fixture.player.as_ref()),
        Some(fixture.player.as_ref())
    ));
    // Exercise the actual persisted-record activation path into a new object.
    chunk.live.store(false, Relaxed);
    world.level.mark_chunks_as_newly_watched(&[position]).await;
    world.activate_entity_chunk(&chunk, None).await;
    let replacement = world
        .get_entity_by_uuid(old.get_entity().entity_uuid)
        .unwrap();
    assert!(!std::ptr::eq(replacement.get_entity(), old.get_entity()));
    let new_dragon = replacement
        .cast_any()
        .downcast_ref::<EnderDragonEntity>()
        .unwrap();
    assert_eq!(new_dragon.current_phase(), EnderDragonPhase::SitScanning);
    assert!(
        new_dragon
            .parts()
            .iter()
            .all(|part| part.current_parent().is_some())
    );
    assert!(!retained.damage_with_context(
        retained.as_ref(),
        8.0,
        DamageType::PLAYER_ATTACK,
        None,
        Some(fixture.player.as_ref()),
        Some(fixture.player.as_ref())
    ));
    assert_eq!(
        replacement.get_living_entity().unwrap().health.load(),
        200.0
    );
    // A retained unloaded parent still has no normal removal reason. Exact
    // object retention must not remove its same-UUID resident replacement.
    world.remove_entity_with_reason(old.as_ref(), RemovalReason::Killed);
    assert!(world.is_current_entity(replacement.as_ref()));
    assert!(
        world
            .entity_tracker
            .get_tracked_entity(replacement.get_entity().entity_id)
            .is_some()
    );
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn scoped_death_height_and_actual_vertical_clipping_preserve_ordinary_movement() {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let world = fixture.player.world();
    let origin = BlockPos::new(8, 12, 8);
    world
        .level
        .read_chunk_sync(&origin.chunk_position(), |chunk| {
            chunk.heightmap.lock().unwrap().set(
                ChunkHeightmapType::MotionBlocking,
                8,
                8,
                73,
                world.min_y,
            );
            chunk.heightmap.lock().unwrap().set(
                ChunkHeightmapType::WorldSurface,
                8,
                8,
                91,
                world.min_y,
            );
        })
        .unwrap();
    assert_eq!(
        world.dragon_death_target(origin),
        Vector3::new(8.5, 74.0, 8.5)
    );
    assert_eq!(
        world.dragon_death_target(BlockPos::new(160, 12, 160)),
        Vector3::new(160.5, f64::from(world.min_y), 160.5)
    );
    assert_eq!(
        world.dragon_death_target(BlockPos::new(30_000_000, 12, 0)),
        Vector3::new(30_000_000.5, f64::from(world.sea_level + 1), 0.5)
    );
    let floor = BlockPos::new(0, 64, 0);
    world
        .level
        .read_chunk_sync(&floor.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(0, 64, 0, Block::STONE.default_state.id);
        })
        .unwrap();
    let cow = from_type(
        &EntityType::COW,
        Vector3::new(0.5, 65.0, 0.5),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(cow.clone()));
    cow.get_entity()
        .move_entity(cow.as_ref(), Vector3::new(0.0, -1.0, 0.0));
    assert_eq!(cow.get_entity().pos.load().y, 65.0);
    assert!(cow.get_entity().vertical_collision.load(Relaxed));
    assert!(cow.get_entity().on_ground.load(Relaxed));
    cow.get_entity()
        .move_entity(cow.as_ref(), Vector3::new(0.0, 1.0, 0.0));
    assert_eq!(cow.get_entity().pos.load().y, 66.0);
    assert!(!cow.get_entity().vertical_collision.load(Relaxed));
    assert!(!cow.get_entity().horizontal_collision.load(Relaxed));
    // Exercise inclusive 3D bounds and each collision through actual Mob ticks.
    for (offset, horizontal, vertical, alive) in [
        (Vector3::new(10.0, 0.0, 0.0), false, false, true),
        (Vector3::new(0.0, 10.0, 0.0), false, false, true),
        (Vector3::new(150.0, 0.0, 0.0), false, false, true),
        (Vector3::new(9.0, 0.0, 0.0), false, false, false),
        (Vector3::new(151.0, 0.0, 0.0), false, false, false),
        (Vector3::new(10.0, 0.0, 0.0), true, false, false),
        (Vector3::new(10.0, 0.0, 0.0), false, true, false),
    ] {
        let live = from_type(
            &EntityType::ENDER_DRAGON,
            world.dragon_death_target(origin) + offset,
            &world,
            Uuid::new_v4(),
        );
        assert!(world.spawn_entity(live.clone()));
        let d = live.cast_any().downcast_ref::<EnderDragonEntity>().unwrap();
        d.mob_entity.set_no_ai(true);
        d.set_fight_origin(origin);
        d.set_phase(EnderDragonPhase::Dying);
        live.get_entity()
            .horizontal_collision
            .store(horizontal, Relaxed);
        live.get_entity()
            .vertical_collision
            .store(vertical, Relaxed);
        live.tick(live.as_ref(), &fixture.server);
        assert_eq!(
            d.mob_entity.living_entity.health.load(),
            if alive { 1.0 } else { 0.0 }
        );
        let mut saved = NbtCompound::new();
        EntityBase::write_nbt(live.as_ref(), &mut saved);
        assert_eq!(saved.get_int("DragonDeathTime"), Some(0));
        world.remove_entity(live.as_ref());
    }
    close_fixture(&fixture).await;
}

#[tokio::test]
async fn removal_reason_is_first_wins_for_ordinary_resident_and_unpublished_cleanup() {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let world = fixture.player.world();
    let cow = from_type(
        &EntityType::COW,
        Vector3::new(8.0, 65.0, 8.0),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(cow.clone()));
    world.remove_entity(cow.as_ref());
    world.remove_entity_with_reason(cow.as_ref(), RemovalReason::Killed);
    assert!(cow.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    assert!(world.get_entity_by_id(cow.get_entity().entity_id).is_none());
    assert!(
        world
            .entity_tracker
            .get_tracked_entity(cow.get_entity().entity_id)
            .is_none()
    );
    let unpublished = from_type(
        &EntityType::COW,
        Vector3::new(8.0, 65.0, 8.0),
        &world,
        Uuid::new_v4(),
    );
    world.remove_entity(unpublished.as_ref());
    assert!(unpublished.get_entity().is_removed());
    assert!(unpublished.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    world.remove_entity(fixture.player.as_ref());
    assert!(fixture.player.get_entity().is_removed());
    assert!(fixture.player.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    // The existing generic remover does not own player registry/session cleanup.
    assert!(world.get_player_by_id(fixture.player.entity_id()).is_some());
    close_fixture(&fixture).await;
}
