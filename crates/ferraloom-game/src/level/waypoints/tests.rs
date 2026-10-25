use super::*;
use crate::entity::{EntityState, ai::attributes::AttributeMap, player::GameType};
use ferraloom_data::{EntityType, registries::attribute::Id as Attribute};
use ferraloom_protocol::{codec::Wire, play::TeamColor};
use ferraloom_util::{ChunkPos, Uuid, Vec3};

const VECTORS: &str = include_str!("../../../tests/java/waypoints/waypoints.tsv");
fn set_range(level: &mut Level, player: EntityRef, attribute: Attribute, value: f64) {
    level
        .entity_mut::<LivingEntity>(player)
        .unwrap()
        .attributes
        .get_instance(attribute)
        .unwrap()
        .set_base_value(value);
}
fn fixture(reciprocal: bool, source_x: f64) -> (Level, Vec<EntityRef>) {
    let mut level = crate::entity::tests::level(false);
    let mut players = Vec::new();
    for index in 1..=3 {
        let entity = level.next_entity_ref();
        let mut player = ServerPlayer::new(
            Uuid::new(1, index),
            format!("observer{index}"),
            index as i32,
            Vec3::new(
                if index == 2 { source_x } else { 8.5 },
                64.0,
                7.5 + index as f64,
            ),
            GameType::Survival,
        );
        player.attributes = AttributeMap::new(EntityType::PLAYER);
        player
            .attributes
            .get_instance(Attribute::WAYPOINT_RECEIVE_RANGE)
            .unwrap()
            .set_base_value(if reciprocal || index == 1 {
                1_000_000.0
            } else {
                0.0
            });
        player
            .attributes
            .get_instance(Attribute::WAYPOINT_TRANSMIT_RANGE)
            .unwrap()
            .set_base_value(if reciprocal || index == 2 {
                1_000_000.0
            } else {
                0.0
            });
        level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .unwrap();
        // Match the Java fixture's controlled view after ChunkMap#addEntity
        // has installed the live requested-distance view.
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.chunk_tracking_center = Some(ChunkPos::new(0, 0));
        player.chunk_tracking_distance = Some(2);
        level.entity_mut::<EntityData>(entity).unwrap().first_tick = false;
        players.push(entity);
    }
    // The Java harness starts at ServerWaypointManager, before publication hooks.
    level.region_mut().waypoint_manager = ServerWaypointManager::default();
    level.take_effects();
    (level, players)
}
fn packets(level: &mut Level) -> String {
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            if let LevelEffect::PlayerPacket { recipients, packet } = effect
                && let cb::Packet::Waypoint(packet) = *packet
            {
                assert_eq!(recipients.len(), 1);
                let receiver = level
                    .entity::<EntityData>(recipients[0])
                    .unwrap()
                    .network_id;
                let wire: String = packet
                    .to_bytes()
                    .unwrap()
                    .iter()
                    .map(|value| format!("{value:02x}"))
                    .collect();
                Some(format!("{receiver}:{wire}"))
            } else {
                None
            }
        })
        .collect();
    if packets.is_empty() {
        "-".to_owned()
    } else {
        packets.join("|")
    }
}
fn apply(level: &mut Level, players: &[EntityRef], input: &str) {
    let args: Vec<_> = input.split(',').collect();
    let player = args
        .get(1)
        .and_then(|arg| arg.parse::<usize>().ok())
        .map(|index| players[index - 1]);
    match args[0] {
        "add" => level.add_waypoint_player(player.unwrap()).unwrap(),
        "track" => level.track_waypoint(player.unwrap()).unwrap(),
        "update" => level.update_waypoint(player.unwrap()).unwrap(),
        "receiver" => level.update_waypoint_player(player.unwrap()).unwrap(),
        "untrack" => level.untrack_waypoint(player.unwrap()).unwrap(),
        "remove" => level.remove_waypoint_player(player.unwrap()).unwrap(),
        "remake" => level.remake_waypoint_connections(player.unwrap()).unwrap(),
        "move" => level
            .set_entity_pos(
                player.unwrap(),
                Vec3::new(
                    args[2].parse().unwrap(),
                    args[3].parse().unwrap(),
                    args[4].parse().unwrap(),
                ),
            )
            .unwrap(),
        "view" => {
            let receiver = level.entity_mut::<ServerPlayer>(player.unwrap()).unwrap();
            receiver.chunk_tracking_center = Some(ChunkPos::new(
                args[2].parse().unwrap(),
                args[3].parse().unwrap(),
            ));
            receiver.chunk_tracking_distance = Some(args[4].parse().unwrap());
            level.update_waypoint_player(player.unwrap()).unwrap();
        }
        "transmit" => set_range(
            level,
            player.unwrap(),
            Attribute::WAYPOINT_TRANSMIT_RANGE,
            args[2].parse().unwrap(),
        ),
        "receive" => set_range(
            level,
            player.unwrap(),
            Attribute::WAYPOINT_RECEIVE_RANGE,
            args[2].parse().unwrap(),
        ),
        "spectator" => {
            level
                .entity_mut::<ServerPlayer>(player.unwrap())
                .unwrap()
                .game_mode = if args[2] == "true" {
                GameType::Spectator
            } else {
                GameType::Survival
            };
            level.remake_waypoint_connections(player.unwrap()).unwrap();
            level.update_waypoint_player(player.unwrap()).unwrap();
        }
        "first" => {
            level
                .entity_mut::<EntityData>(player.unwrap())
                .unwrap()
                .first_tick = args[2].parse().unwrap()
        }
        "rule" => {
            let enabled = args[1].parse().unwrap();
            level.rules_mut().locator_bar = enabled;
            level.update_locator_bar(enabled).unwrap();
        }
        "team" => {
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .add_player_team("waypoints");
            let color = match args[2] {
                "BLACK" => TeamColor::Black,
                "RED" => TeamColor::Red,
                "GREEN" => TeamColor::Green,
                _ => panic!("native color"),
            };
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .update_team("waypoints", |team| team.color = Some(color))
                .unwrap();
            level
                .shared_mut()
                .unwrap()
                .scoreboard
                .add_player_to_team(&format!("observer{}", args[1]), "waypoints")
                .unwrap();
            level.remake_waypoint_connections(player.unwrap()).unwrap();
        }
        "icon" => {
            level
                .entity_mut::<LivingEntity>(player.unwrap())
                .unwrap()
                .locator_bar_icon
                .color = Some(ferraloom_protocol::play::RgbColor(
                u32::from_str_radix(args[2], 16).unwrap(),
            ));
            level.remake_waypoint_connections(player.unwrap()).unwrap();
        }
        "vehicle" => {
            level
                .entity::<EntityData>(players[0])
                .unwrap()
                .vehicle
                .set(Some(players[1]));
            level.update_waypoint(players[1]).unwrap();
        }
        "passenger" => {
            level
                .entity_mut::<EntityData>(players[1])
                .unwrap()
                .passengers
                .push(players[0]);
            level.update_waypoint(players[1]).unwrap();
        }
        _ => panic!("native operation {input}"),
    }
}

#[test]
fn native_manager_connections_and_packet_bytes_keep_update_replacement_and_table_order() {
    let mut current = String::new();
    let mut fixture = None;
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("manager\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[1] != current {
            fixture = Some(self::fixture(
                fields[2].parse().unwrap(),
                fields[3].parse().unwrap(),
            ));
            current = fields[1].to_owned();
        }
        let (level, players) = fixture.as_mut().unwrap();
        apply(level, players, fields[4]);
        assert_eq!(packets(level), fields[5], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 54);
}
#[test]
fn native_icon_codec_preserves_explicit_colors_and_rejects_invalid_fields() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("icon\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let tag = ferraloom_nbt::parse(fields[1]).unwrap();
        let result = load_icon(Some(&tag));
        if fields[2] == "-" {
            assert!(result.is_err(), "{row}");
        } else {
            assert_eq!(
                save_icon(&result.unwrap()),
                ferraloom_nbt::parse(fields[2]).unwrap(),
                "{row}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 15);
}
