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
