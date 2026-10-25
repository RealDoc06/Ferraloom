use super::*;
use crate::{
    Behaviors,
    entity::{
        item::{ItemEntity, PrimedTnt},
        player::GameType,
        synched_data,
    },
    level::{Region, chunk::LevelChunk},
};
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_protocol::{codec::VarInt, component::ItemStack};
use ferraloom_util::{ChunkPos, Uuid, Vec3};
use std::sync::Arc;

fn level() -> Level {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    {
        let mut fixture_level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    }
}
fn item(level: &mut Level) -> EntityRef {
    let id = level.next_entity_ref();
    let mut item = ItemEntity::new(level.next_entity_id().unwrap(), 1);
    item.parent.set_pos(Vec3::new(0.0, 64.0, 0.0));
    item.set_item(ItemStack::new(
        ferraloom_data::Item::STONE,
        3,
        Default::default(),
    ));
    level
        .add_entity(id, EntityState::Item(Box::new(item)))
        .unwrap();
    id
}
fn player(level: &mut Level, position: Vec3) -> EntityRef {
    let id = level.next_entity_ref();
    let mut player = ServerPlayer::new(
        Uuid::new(0, id.0 as i64),
        "viewer".into(),
        level.next_entity_id().unwrap(),
        position,
        GameType::Spectator,
    );
    player.view_distance = 10;
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    id
}
fn view(player: EntityRef) -> PlayerTrackingView {
    PlayerTrackingView {
        player,
        server_view_distance: 10,
        broadcast_range_percentage: 100,
        chunk_tracked: true,
    }
}

#[test]
fn tracking_range_boundary_is_inclusive_ignores_height_and_requires_the_sent_chunk() {
    let mut level = level();
    let item = item(&mut level);
    let viewer = player(&mut level, Vec3::new(32.0, 100000.0, 0.0));
    level
        .entity_mut::<ServerPlayer>(viewer)
        .unwrap()
        .view_distance = 2;
    let mut tracked = TrackedEntity::new(item, &level).unwrap();
    let paired = tracked.update_player(&mut level, view(viewer)).unwrap();
    assert_eq!(paired.len(), 1);
    assert!(matches!(paired[0].message, ClientboundMessage::Bundle(_)));
    assert!(
        tracked
            .update_player(&mut level, view(viewer))
            .unwrap()
            .is_empty()
    );
    level
        .set_entity_pos(
            viewer,
            Vec3::new(f64::from_bits(32.0_f64.to_bits() + 1), 100000.0, 0.0),
        )
        .unwrap();
    let removed = tracked.update_player(&mut level, view(viewer)).unwrap();
    assert_eq!(removed.len(), 1);
    assert!(
        matches!(&removed[0].message,ClientboundMessage::Packet(packet) if matches!(packet.as_ref(),Packet::RemoveEntities(_)))
    );
    assert!(tracked.seen_by().is_empty());
    level
        .set_entity_pos(viewer, Vec3::new(32.0, 100000.0, 0.0))
        .unwrap();
    let mut hidden = view(viewer);
    hidden.chunk_tracked = false;
    assert!(
        tracked
            .update_player(&mut level, hidden)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        tracked
            .update_player(&mut level, view(viewer))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(tracked.remove_player(&mut level, viewer).unwrap().len(), 1);
    assert!(
        tracked
            .remove_player(&mut level, viewer)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn tracking_range_includes_transitive_passengers_before_server_percentage_scaling() {
    let mut level = level();
    let item = item(&mut level);
    let viewer = player(&mut level, Vec3::new(100.0, 64.0, 0.0));
    let rider = player(&mut level, Vec3::new(0.0, 64.0, 0.0));
    let mount = level.next_entity_ref();
    let tnt = PrimedTnt::new(level.next_entity_id().unwrap(), 2);
    level
        .add_entity(mount, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    let mut tracked = TrackedEntity::new(item, &level).unwrap();
    let mut settings = view(viewer);
    settings.broadcast_range_percentage = 50;
    assert!(
        tracked
            .update_player(&mut level, settings)
            .unwrap()
            .is_empty()
    );
    assert!(crate::entity::start_riding(mount, item, true, &mut level).unwrap());
    assert!(crate::entity::start_riding(rider, mount, true, &mut level).unwrap());
    assert_eq!(
        tracked.update_player(&mut level, settings).unwrap().len(),
        1
    );
    crate::entity::stop_riding(rider, &mut level).unwrap();
    assert_eq!(
        tracked.update_player(&mut level, settings).unwrap().len(),
        1
    );
    assert!(tracked.seen_by().is_empty());
}

#[test]
fn dirty_metadata_waits_for_the_chunk_map_gate_but_needs_sync_opens_it() {
    let mut level = level();
    let item = item(&mut level);
    let viewer = player(&mut level, Vec3::new(1.0, 64.0, 0.0));
    let mut tracked = TrackedEntity::new(item, &level).unwrap();
    let settings = view(viewer);
    tracked.update_player(&mut level, settings).unwrap();
    assert!(tracked.tick(&mut level, &[settings]).unwrap().is_empty());
    assert!(
        level
            .entity::<EntityData>(item)
            .unwrap()
            .entity_data
            .is_dirty()
    );
    level.entity_mut::<EntityData>(item).unwrap().needs_sync = true;
    assert!(!tracked.tick(&mut level, &[settings]).unwrap().is_empty());
    assert!(
        !level
            .entity::<EntityData>(item)
            .unwrap()
            .entity_data
            .is_dirty()
    );
    level
        .entity_mut::<EntityData>(item)
        .unwrap()
        .entity_data
        .set(synched_data::AIR_SUPPLY, VarInt(150));
    assert!(tracked.tick(&mut level, &[settings]).unwrap().is_empty());
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::ZERO,
            -4..20,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        ))
        .unwrap();
    level
        .set_chunk_entity_ticking(ChunkPos::ZERO, true)
        .unwrap();
    let packets = tracked.tick(&mut level, &[settings]).unwrap();
    assert!(packets.iter().any(|message| matches!(&message.message,ClientboundMessage::Packet(packet) if matches!(packet.as_ref(),Packet::SetEntityData(_)))));
}
