use super::*;
use crate::{
    Behaviors,
    entity::{EntityState, player::GameType},
    level::{LevelContext, Region, effects::LevelEffect},
};
use ferraloom_protocol::play::clientbound as cb;
use ferraloom_util::{Uuid, Vec3};

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn add_player(level: &mut Level, position: Vec3, mode: GameType) -> EntityRef {
    if level.shared().is_err() {
        level.install_shared(Box::default());
    }
    let entity = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(0, 1),
        "tick".into(),
        level.next_entity_id(),
        position,
        mode,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    entity
}
fn vector(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}

#[test]
fn loaded_neighbor_guard_matches_native_expanded_bounds_at_chunk_edges() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("player-loaded-neighbors\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let x = f64::from_bits(u64::from_str_radix(fields[1], 16).unwrap());
        let mut level = crate::entity::tests::level(false);
        let player = add_player(&mut level, Vec3::new(x, 64.0, 8.5), GameType::Spectator);
        assert_eq!(
            touching_unloaded_chunk(player, &level).unwrap().to_string(),
            fields[2],
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 13);
}

#[test]
fn spectator_without_loaded_chunks_still_syncs_xp_without_running_the_parent_tick() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let player = add_player(&mut level, Vec3::new(8.5, 64.0, 8.5), GameType::Spectator);
    level.entity_mut::<ServerPlayer>(player).unwrap().tick_count = 1;
    level.take_effects();
    player_do_tick_with_tick_rate(player, &mut level, &TickRateManager::default()).unwrap();
    let state = level.entity::<ServerPlayer>(player).unwrap();
    assert_eq!(state.tick_count, 1);
    assert_eq!(state.connection_ticks, 0);
    assert!(state.first_tick);
    let output = level.take_effects();
    assert_eq!(output.len(), 2);
    assert!(
        matches!(&output[1], LevelEffect::PlayerPacket { recipients, packet } if recipients == &[player] && matches!(packet.as_ref(), cb::Packet::SetExperience(_)))
    );
    player_do_tick_with_tick_rate(player, &mut level, &TickRateManager::default()).unwrap();
    assert!(level.take_effects().is_empty());
}

#[test]
fn listener_frame_preserves_native_callback_clock_and_first_good_snap_semantics() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("player-connection-frame\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        let entity = add_player(&mut level, Vec3::new(8.5, 64.0, 8.5), GameType::Survival);
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.connection_ticks = 9;
        player.velocity = Vec3::new(0.25, -0.5, 0.125);
        player.reset_position();
        player.old_position = player.position;
        let called = format!(
            "{}:{}",
            player.connection_ticks,
            vector(player.old_position)
        );
        level
            .set_entity_pos(entity, Vec3::new(11.0, 70.0, 12.0))
            .unwrap();
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        if fields[1] == "true" {
            player.first_good = Vec3::new(4e7, 80.0, -4e7);
            player.yaw = -400.0;
            player.pitch = 100.0;
        } else {
            player.yaw = 431.0;
            player.pitch = -120.0;
        }
        let (position, yaw, pitch) = (player.first_good, player.yaw, player.pitch);
        crate::entity::teleport::abs_snap_to(entity, position, yaw, pitch, &mut level).unwrap();
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.finish_connection_tick();
        let actual = format!(
            "{called}\t{}\t{}\t{:x},{:x},{:x},{:x}\t{}\t{}",
            vector(player.position),
            vector(player.old_position),
            player.yaw.to_bits(),
            player.pitch.to_bits(),
            player.old_yaw.to_bits(),
            player.old_pitch.to_bits(),
            vector(player.velocity),
            player.connection_ticks
        );
        assert_eq!(actual, fields[2..].join("\t"), "{row}");
        count += 1;
    }
    assert_eq!(count, 2);
}

fn packet_text(packet: &cb::Packet) -> String {
    match packet {
        cb::Packet::SetHealth(p) => format!(
            "health:{:x}:{}:{:x}",
            p.health.to_bits(),
            p.food.0,
            p.saturation.to_bits()
        ),
        cb::Packet::SetScore(p) => format!("score:{}:{}", p.objective_name, p.score.0),
        cb::Packet::SetExperience(p) => format!(
            "xp:{}:{}:{:x}",
            p.experience_level.0,
            p.total_experience.0,
            p.experience_progress.to_bits()
        ),
        _ => panic!("unexpected player sync packet {packet:?}"),
    }
}
#[test]
fn native_do_tick_orders_health_scores_and_experience_for_each_connection() {
    use crate::scoreboard::{Objective, ObjectiveCriteria};
    use ferraloom_protocol::{chat::Component, play::DisplaySlot};
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let id = add_player(&mut level, Vec3::new(8.5, 64.0, 8.5), GameType::Spectator);
    let slots = [
        DisplaySlot::List,
        DisplaySlot::Sidebar,
        DisplaySlot::BelowName,
        DisplaySlot::TeamBlack,
        DisplaySlot::TeamDarkBlue,
        DisplaySlot::TeamDarkGreen,
    ];
    for (criterion, slot) in ["health", "food", "air", "armor", "xp", "level"]
        .into_iter()
        .zip(slots)
    {
        let board = &mut level.shared_mut().unwrap().scoreboard;
        board
            .add_objective(
                criterion.into(),
                Objective::new(
                    ObjectiveCriteria::by_name(criterion).unwrap(),
                    Component::literal(criterion),
                ),
            )
            .unwrap();
        board
            .set_display_objective(slot, Some(criterion.into()))
            .unwrap();
    }
    level.shared_mut().unwrap().scoreboard.drain_packets();
    let p = level.entity_mut::<ServerPlayer>(id).unwrap();
    p.tick_count = 1;
    p.set_health(17.25);
    p.entity_data.set(data::PLAYER_ABSORPTION, 2.125);
    p.attributes
        .get_instance(Attribute::ARMOR)
        .unwrap()
        .set_base_value(2.5);
    p.total_experience = 123;
    p.experience_level = 7;
    p.experience_progress = 0.25;
    level.take_effects();
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("player-sync-order\t"))
    {
        let row: Vec<_> = row.split('\t').collect();
        let p = level.entity_mut::<ServerPlayer>(id).unwrap();
        match row[1] {
            "2" => p.food_data.set_saturation(2.5),
            "3" => {
                p.total_experience += 1;
                p.experience_progress = 0.5;
            }
            "4" => {
                p.set_health(12.0);
                p.entity_data.set(data::PLAYER_ABSORPTION, 0.0);
                p.food_data.set_food_level(7);
                p.food_data.set_saturation(0.0);
                p.entity_data.set(data::AIR_SUPPLY, VarInt(8));
                p.attributes
                    .get_instance(Attribute::ARMOR)
                    .unwrap()
                    .set_base_value(5.99);
                p.total_experience = 24;
                p.experience_level = 8;
            }
            _ => {}
        }
        player_do_tick_with_tick_rate(id, &mut level, &TickRateManager::default()).unwrap();
        let mut own = Vec::new();
        let mut other = Vec::new();
        for effect in level.take_effects() {
            match effect {
                LevelEffect::PlayerPacket { recipients, packet } => {
                    assert_eq!(recipients, [id]);
                    own.push(packet_text(&packet));
                }
                LevelEffect::BroadcastAll(packet) => {
                    let text = packet_text(&packet);
                    own.push(text.clone());
                    other.push(text);
                }
                _ => panic!("unexpected player sync effect"),
            }
        }
        let joined = |v: Vec<String>| {
            if v.is_empty() {
                "-".into()
            } else {
                v.join(";")
            }
        };
        assert_eq!(joined(own), row[2], "own {row:?}");
        assert_eq!(joined(other), row[3], "other {row:?}");
        assert!(
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .drain_packets()
                .is_empty()
        );
    }
}
