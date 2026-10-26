use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        player::{GameType, ServerPlayer},
        synched_data::NO_GRAVITY,
    },
    level::{distance_manager::FullChunkStatus, effects::LevelEffect},
};
use ferraloom_protocol::play::clientbound::Packet;
use ferraloom_util::{ChunkPos, Uuid};
use std::collections::BTreeMap;

const VECTORS: &str = include_str!("../../../../tests/java/entity/powder/vectors.tsv");

fn vector(value: &str) -> Vec3 {
    let bits: Vec<_> = value
        .split(',')
        .map(|v| f64::from_bits(u64::from_str_radix(v, 16).unwrap()))
        .collect();
    Vec3::new(bits[0], bits[1], bits[2])
}
fn vector_bits(value: Vec3) -> [u64; 3] {
    [value.x.to_bits(), value.y.to_bits(), value.z.to_bits()]
}
fn position(value: BlockPos) -> String {
    format!("{},{},{}", value.x, value.y, value.z)
}
fn states(value: &str) -> BTreeMap<String, (BlockPos, BlockState)> {
    value
        .split(';')
        .filter(|s| *s != "-")
        .map(|s| {
            let fields: Vec<i32> = s.split(',').map(|v| v.parse().unwrap()).collect();
            let pos = BlockPos::new(fields[0], fields[1], fields[2]);
            (
                position(pos),
                (pos, BlockState::from_id(fields[3] as u16).unwrap()),
            )
        })
        .collect()
}

/// Replay the actual native tick with noPhysics/noGravity on both sides. This
/// isolates the powder branch while still executing real clip and onLand code.
#[test]
fn native_powder_tick_water_interception_and_post_packet_solidification() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut checked = 0;
    for row in VECTORS.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
        level.set_unported_behavior(UnportedBehavior::Strict);
        level
            .set_chunk_loading_status(ChunkPos::ZERO, FullChunkStatus::BlockTicking, true)
            .unwrap();
        let mut world = states(fields[6]);
        for &(pos, state) in world.values() {
            level.set_block_state(pos, state).unwrap();
        }

        let viewer = level.next_entity_ref();
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "powder_viewer".into(),
            2,
            Vec3::new(12.5, 65.0, 8.5),
            GameType::Spectator,
        );
        player.chunk_tracking_view.insert(ChunkPos::ZERO);
        level
            .add_entity(viewer, EntityState::Player(Box::new(player)))
            .unwrap();
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .remove(&ChunkPos::ZERO);

        let entity = level.next_entity_ref();
        let mut falling = FallingBlockEntity::new(1, 0);
        falling.block_state = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        falling.drop_item = false;
        falling.parent.set_pos(vector(fields[3]));
        falling.parent.old_position = falling.parent.position;
        falling.parent.velocity = vector(fields[4]);
        falling.parent.on_ground = fields[5].parse().unwrap();
        falling.parent.no_physics = true;
        falling.parent.entity_data.set(NO_GRAVITY, true);
        level
            .add_entity(entity, EntityState::FallingBlock(Box::new(falling)))
            .unwrap();
        level.take_effects();

        FallingBlockBehavior
            .tick(entity, &mut level)
            .unwrap_or_else(|error| panic!("{row}: {error:?}"));
        let falling = level.entity::<FallingBlockEntity>(entity).unwrap();
        assert_eq!(
            vector_bits(falling.parent.position),
            vector_bits(vector(fields[7])),
            "position {row}"
        );
        assert_eq!(
            vector_bits(falling.parent.velocity),
            vector_bits(vector(fields[8])),
            "velocity {row}"
        );
        assert_eq!(falling.parent.on_ground.to_string(), fields[9], "{row}");
        assert_eq!(falling.parent.removed.to_string(), fields[10], "{row}");
        assert_eq!(falling.time.to_string(), fields[11], "{row}");
        assert_eq!(falling.block_state.id().to_string(), fields[12], "{row}");

        let mut trace = Vec::new();
        for effect in level.take_effects() {
            match effect {
                LevelEffect::BlockUpdated { pos, new, .. } => {
                    trace.push(format!("set:{}:{}", position(pos), new.id()));
                    world.insert(position(pos), (pos, new));
                }
                LevelEffect::PlayerPacket { recipients, packet } => match *packet {
                    Packet::BlockUpdate(packet) => {
                        assert_eq!(recipients, vec![viewer], "{row}");
                        trace.push(format!(
                            "block:{}:{}",
                            position(packet.pos),
                            packet.block_state.0
                        ));
                    }
                    Packet::AddTransientBlock(packet) => {
                        assert_eq!(recipients, vec![viewer], "{row}");
                        trace.push(format!(
                            "transient:{}:{}",
                            position(packet.pos),
                            packet.block_state.0
                        ));
                    }
                    Packet::RemoveEntities(packet)
                        if packet.entity_ids.iter().any(|id| id.0 == 1) =>
                    {
                        assert_eq!(recipients, vec![viewer], "{row}");
                        trace.push("remove".into());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        assert_eq!(trace.join(";"), fields[14], "effect order {row}");
        let actual = world
            .values()
            .map(|&(pos, _)| {
                format!(
                    "{},{}",
                    position(pos),
                    level.get_block_state(pos).unwrap().id()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            if actual.is_empty() { "-" } else { &actual },
            fields[13],
            "world {row}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0, "{row}");
        checked += 1;
    }
    assert_eq!(checked, 42);
}
