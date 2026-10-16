use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

/// Mined statistics identify blocks, even when the world contains different
/// states of one block. The distinct block is a negative control for collapsing
/// all mining into one counter or confusing state IDs with block IDs.
#[test]
fn mined_block_identity_is_shared_across_states_but_not_other_blocks() {
    use pumpkin_data::Block;
    let oak = [&Block::OAK_LOG.states[0], &Block::OAK_LOG.states[1]];
    let birch = &Block::BIRCH_LOG.states[0];
    assert_ne!(oak[0].id, oak[1].id);
    assert_ne!(oak[0].id.as_u16(), Block::OAK_LOG.id.as_u16());
    let mut stats = Statistics::default();
    for state in [oak[0], oak[1], birch] {
        let block = Block::from_state_id(state.id);
        stats.increment(StatisticCategory::Mined, i32::from(block.id.as_u16()), 1);
    }
    let oak_id = registry::stat_id(StatisticCategory::Mined, "minecraft:oak_log").unwrap();
    let birch_id = registry::stat_id(StatisticCategory::Mined, "minecraft:birch_log").unwrap();
    assert_ne!(oak_id, birch_id);
    assert_eq!(stats.get(StatisticCategory::Mined, oak_id), 2);
    assert_eq!(stats.get(StatisticCategory::Mined, birch_id), 1);
    assert_eq!(stats.network_entries().count(), 2);
}

/// Full ID/name coverage is pinned to the official report, without embedding
/// its multi-megabyte contents. Hash input is specified in the witness file.
#[test]
fn all_nine_value_registries_match_official_26_3_and_reject_invalid_ids() {
    #[derive(Deserialize)]
    struct Registry {
        count: i32,
        sha256: String,
    }
    #[derive(Deserialize)]
    struct Witness {
        registries: BTreeMap<String, Registry>,
    }
    let witness: Witness = serde_json::from_str(include_str!("registry-witness.json")).unwrap();
    let bindings = [
        "block",
        "item",
        "item",
        "item",
        "item",
        "item",
        "entity_type",
        "entity_type",
        "custom_stat",
    ];
    let mut category_rows = String::new();
    for (id, binding) in (0..9).zip(bindings) {
        let category = StatisticCategory::from_i32(id).unwrap();
        let name = registry::category_name(category);
        assert_eq!(registry::category(name), Some(category));
        category_rows.push_str(&format!("{id}\0{name}\n"));
        let proof = &witness.registries[binding];
        let mut rows = String::new();
        for raw in 0..proof.count {
            let key = registry::stat_key(category, raw).unwrap();
            let name = format!("minecraft:{key}");
            assert_eq!(registry::stat_id(category, &name), Some(raw));
            rows.push_str(&format!("{raw}\0{name}\n"));
        }
        assert_eq!(
            hex::encode(Sha256::digest(rows.as_bytes())),
            proof.sha256,
            "{name}"
        );
        for invalid in [
            -1,
            proof.count,
            i32::from(u16::MAX),
            65_536,
            i32::MAX,
            i32::MIN,
        ] {
            assert_eq!(
                registry::stat_key(category, invalid),
                None,
                "{name}/{invalid}"
            );
            let mut live = Statistics::default();
            live.set(category, invalid, 8);
            live.increment(category, invalid, 3);
            assert_eq!(
                live.network_entries().count(),
                0,
                "invalid numeric IDs must never enter award_stats"
            );
        }
        for unknown in [
            "other:stone",
            "minecraft:nonexistent_statistic",
            "stone",
            "minecraft:minecraft:stone",
        ] {
            assert_eq!(registry::stat_id(category, unknown), None);
        }
    }
    assert_eq!(witness.registries["stat_type"].count, 9);
    assert_eq!(
        hex::encode(Sha256::digest(category_rows.as_bytes())),
        witness.registries["stat_type"].sha256
    );
    for id in [-1, 9, i32::MAX] {
        assert_eq!(StatisticCategory::from_i32(id), None);
    }
    assert_eq!(registry::category("other:custom"), None);
}

#[test]
fn failed_reload_is_transactional_and_disables_saving() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("player.json");
    let mut live = Statistics::default();
    assert!(matches!(
        live.snapshot(),
        Err(StatisticsDataError::NotLoaded)
    ));
    live.load(&path, true).unwrap();
    live.increment_custom(CustomStatistic::OpenBarrel, 18);
    for bytes in [
        br#"{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":99,"minecraft:open_chest":1.5}}}"#.as_slice(),
        br#"{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":1,"minecraft:open_barrel":2}}}"#,
        br#"{"DataVersion":5023,"stats":{},"extension":{"a":1,"a":2}}"#,
        br#"{"DataVersion":5023,"stats":{},"stats":{}}"#,
        br#"{"DataVersion":5023,"DataVersion":5023,"stats":{}}"#,
        br#"{"DataVersion":5022,"stats":{}}"#,
        br#"{"DataVersion":5024,"stats":{}}"#,
        br#"{"stats":{}}"#,
        br#"{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":2147483648}}}"#,
        br#"{"DataVersion":5023,"stats":{"custom":{"open_barrel":1}}}"#,
        b"incomplete JSON",
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(live.load(&path, true).is_err());
        assert_eq!(live.get(StatisticCategory::Custom, CustomStatistic::OpenBarrel as i32), 18);
        assert!(matches!(live.snapshot(), Err(StatisticsDataError::NotLoaded)));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn private_history_is_not_imported_merged_or_rewritten() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("player.json");
    let canonical =
        br#"{"DataVersion":5023,"stats":{"minecraft:custom":{"minecraft:open_barrel":18}}}"#;
    let mut history = NbtCompound::new();
    history.put_int("8:63", 18);
    for payload in [NbtTag::Compound(history), NbtTag::Int(0)] {
        for coexist in [false, true] {
            if coexist {
                fs::write(&path, canonical).unwrap();
            }
            let mut nbt = NbtCompound::new();
            nbt.put("Statistics", payload.clone());
            let mut live = Statistics::default();
            live.read_nbt(&nbt);
            assert!(matches!(
                live.load(&path, true),
                Err(StatisticsDataError::LegacyPrivateStatistics)
            ));
            assert!(matches!(
                live.snapshot(),
                Err(StatisticsDataError::NotLoaded)
            ));
            assert_eq!(live.network_entries().count(), 0);
            let mut written = NbtCompound::new();
            live.write_nbt(&mut written);
            assert_eq!(written.get("Statistics"), Some(&payload));
            if coexist {
                assert_eq!(fs::read(&path).unwrap(), canonical);
                fs::remove_file(&path).unwrap();
            } else {
                assert!(!path.exists());
            }
        }
    }
    let mut live = Statistics::default();
    let mut nbt = NbtCompound::new();
    nbt.put_compound("Statistics", NbtCompound::new());
    live.read_nbt(&nbt);
    live.load(&path, true).unwrap();
    live.increment_custom(CustomStatistic::OpenBarrel, 2);
    let mut written = NbtCompound::new();
    live.write_nbt(&mut written);
    assert_eq!(written.get("Statistics"), nbt.get("Statistics"));
    fs::write(&path, live.snapshot().unwrap().unwrap()).unwrap();
    live.read_nbt(&written);
    live.load(&path, true).unwrap();
    assert_eq!(
        live.get(
            StatisticCategory::Custom,
            CustomStatistic::OpenBarrel as i32
        ),
        2
    );
}
