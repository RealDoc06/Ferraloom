use super::*;
use ferraloom_protocol::{Wire, Writer};
use serde_json::Value;
fn oracle() -> Value {
    use std::io::Read;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("vanilla.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn wire(packet: AwardStats) -> String {
    let mut writer = Writer::new();
    packet.encode(&mut writer).unwrap();
    writer
        .as_slice()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[test]
fn vanilla_stat_registries_json_and_dirty_packets() {
    let fixture = oracle();
    for row in fixture["types"].as_array().unwrap() {
        let kind = StatType::from_name(row["name"].as_str().unwrap()).unwrap();
        assert_eq!(kind.protocol_id(), row["id"].as_i64().unwrap() as i32);
        for name in row["values"].as_array().unwrap() {
            let stat = kind
                .decode_stat(&Identifier::parse(name.as_str().unwrap()).unwrap())
                .unwrap();
            assert_eq!(stat.stat_type(), kind);
            assert_eq!(stat.value_name(), name.as_str().unwrap());
        }
    }
    for row in fixture["persistence"].as_array().unwrap() {
        let mut counter = ServerStatsCounter::default();
        counter.parse(&row["input"]).unwrap();
        assert_eq!(counter.to_json(), row["saved"], "{}", row["input"]);
        assert_eq!(
            wire(counter.send_stats()),
            row["initial_packet"].as_str().unwrap()
        );
        counter.mark_all_dirty();
        assert_eq!(
            wire(counter.send_stats()),
            row["all_packet"].as_str().unwrap()
        );
        assert_eq!(
            wire(counter.send_stats()),
            row["empty_packet"].as_str().unwrap()
        );
    }
}
#[test]
fn vanilla_counter_arithmetic_same_value_writes_and_requests() {
    let fixture = oracle();
    let mut counter = ServerStatsCounter::default();
    let jump = Stat::Custom(custom::Id::JUMP);
    for row in fixture["steps"].as_array().unwrap() {
        let amount = row["amount"].as_i64().unwrap() as i32;
        if row["set"].as_bool().unwrap() {
            counter.set_value(jump, amount);
        } else {
            counter.increment(jump, amount);
        }
        counter.set_value(Stat::Mined(Block::STONE), 3);
        assert_eq!(
            counter.get_value(jump),
            row["value"].as_i64().unwrap() as i32
        );
        assert_eq!(counter.to_json(), row["saved"]);
        assert_eq!(
            wire(handle_client_command(&mut counter, ClientCommandAction::RequestStats).unwrap()),
            row["packet"].as_str().unwrap()
        );
    }
    assert_eq!(counter.to_json(), fixture["saved_file"]);
    assert!(handle_client_command(&mut counter, ClientCommandAction::PerformRespawn).is_none());
    assert!(matches!(
        counter.parse(&serde_json::json!({"DataVersion":1343})),
        Err(LootError::Unsupported(_))
    ));
}

#[test]
fn vanilla_dirty_packets_follow_jdk25_copy_capacity_boundaries() {
    for row in oracle()["bulk"].as_array().unwrap() {
        let mut counter = ServerStatsCounter::default();
        for entry in row["entries"].as_array().unwrap() {
            let key = StatType::Custom
                .decode_stat(&Identifier::parse(entry["name"].as_str().unwrap()).unwrap())
                .unwrap();
            counter.set_value(key, entry["count"].as_i64().unwrap() as i32);
        }
        assert_eq!(wire(counter.send_stats()), row["packet"].as_str().unwrap());
        let mut restored = ServerStatsCounter::default();
        assert!(restored.parse(&row["saved"]).unwrap().is_empty());
        restored.mark_all_dirty();
        assert_eq!(
            wire(restored.send_stats()),
            row["restored_packet"].as_str().unwrap()
        );
    }
}

#[test]
fn live_stat_counter_and_scoreboard_update_in_native_order() {
    use crate::{
        Behaviors,
        entity::{
            EntityState,
            player::{GameType, ServerPlayer},
        },
        level::{Level, LevelError, Region},
        scoreboard::{Objective, ObjectiveCriteria},
    };
    use ferraloom_protocol::chat::Component;
    use ferraloom_util::{Uuid, Vec3};
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::NIL,
                "Stats".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            ))),
        )
        .unwrap();
    let stat = Stat::Custom(custom::Id::OPEN_CHEST);
    assert_eq!(
        award_stat(&mut level, player, stat, 2),
        Err(LevelError::MissingServerShared)
    );
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .stats
            .get_value(stat),
        2
    );
    level.install_shared(Box::default());
    let mut objective = Objective::new(
        ObjectiveCriteria::by_name(&stat.objective_name()).unwrap(),
        Component::literal("opened"),
    );
    objective.display_auto_update = true;
    level
        .shared_mut()
        .unwrap()
        .scoreboard
        .add_objective("opened".into(), objective)
        .unwrap();
    level
        .shared_mut()
        .unwrap()
        .scoreboard
        .set_score("Stats", "opened", i32::MAX, None, true)
        .unwrap();
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .stats
        .set_value(stat, i32::MAX - 1);
    let display = level.entity_display_name(player).unwrap();
    award_stat(&mut level, player, stat, 3).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .stats
            .get_value(stat),
        i32::MAX
    );
    fn nested_read(level: &Level) -> &crate::scoreboard::Score {
        level
            .shared()
            .unwrap()
            .scoreboard
            .scoreboard()
            .get_player_score("Stats", "opened")
            .unwrap()
    }
    assert_eq!(nested_read(&level).value, i32::MIN + 2);
    assert_eq!(nested_read(&level).display.as_ref(), Some(&display));
    reset_stat(&mut level, player, stat).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .stats
            .get_value(stat),
        0
    );
    assert_eq!(nested_read(&level).value, 0);
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
}
