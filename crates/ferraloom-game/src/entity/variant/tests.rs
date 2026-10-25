use super::*;
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use std::{convert::Infallible, path::Path};

fn resources() -> ResourceManager {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
    ])
}
fn condition_json(condition: &SpawnCondition) -> Value {
    match condition {
        SpawnCondition::Biome(set) => {
            serde_json::json!({"type":"minecraft:biome","biomes":set.to_json().unwrap()})
        }
        SpawnCondition::Structure(set) => {
            serde_json::json!({"type":"minecraft:structure","structures":set.to_json().unwrap()})
        }
        SpawnCondition::MoonBrightness(_) => panic!("vanilla Cow registry has no moon selector"),
    }
}
#[test]
fn mob_variant_registries_match_every_native_bootstrap_key_and_spawn_selector() {
    let variants = MobVariants::load(&resources()).unwrap();
    let rows = include_str!("../../../tests/java/entity/summon/vectors.tsv")
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let native = rows
        .iter()
        .filter(|row| row[0] == "mob-variant")
        .collect::<Vec<_>>();
    assert_eq!(variants.cow.variants.entries.len(), native.len());
    for (actual, expected) in variants.cow.variants.entries.iter().zip(native) {
        assert_eq!(actual.key.as_str(), expected[1]);
        let actual = actual
            .selectors
            .iter()
            .map(|selector| {
                let mut value = serde_json::json!({"priority":selector.priority});
                if let Some(condition) = &selector.condition {
                    value["condition"] = condition_json(condition);
                }
                value
            })
            .collect::<Vec<_>>();
        assert_eq!(
            Value::Array(actual),
            serde_json::from_str::<Value>(expected[2]).unwrap(),
            "{}",
            expected[1]
        );
    }
    assert_eq!(
        variants
            .cow
            .sound_variants
            .iter()
            .map(Identifier::as_str)
            .collect::<Vec<_>>(),
        rows.iter()
            .filter(|row| row[0] == "mob-sound")
            .map(|row| row[1])
            .collect::<Vec<_>>()
    );
}

#[test]
fn priority_selection_preserves_native_stable_ties_duplicate_candidates_and_rng() {
    let mut count = 0;
    for row in include_str!("../../../tests/java/entity/summon/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("variant-select\t"))
    {
        let row = row.split('\t').collect::<Vec<_>>();
        let scenario = row[1].parse::<i32>().unwrap();
        let selector = |id, priority| SpawnSelector {
            priority,
            condition: Some(SpawnCondition::MoonBrightness(DoubleBounds {
                min: Some(f64::from(id)),
                max: None,
            })),
        };
        let mut entries = vec![
            SpawnVariant {
                key: "test:a".parse().unwrap(),
                selectors: vec![selector(0, 5), selector(1, 5)],
            },
            SpawnVariant {
                key: "test:b".parse().unwrap(),
                selectors: vec![selector(2, 5)],
            },
            SpawnVariant {
                key: "test:c".parse().unwrap(),
                selectors: vec![selector(3, 0)],
            },
        ];
        if scenario == 8 {
            entries.clear();
        }
        if scenario == 9 {
            entries.truncate(1);
            entries[0].selectors.truncate(1);
        }
        let registry = SpawnVariantRegistry { entries };
        let mut calls = Vec::new();
        let selected = registry
            .select(|condition| {
                let SpawnCondition::MoonBrightness(range) = condition else {
                    unreachable!()
                };
                let id = range.min.unwrap() as i32;
                calls.push(id);
                Ok::<_, Infallible>(scenario != 9 && (id == 3 || scenario & (1 << id) != 0))
            })
            .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|id| id.path())
                .collect::<Vec<_>>()
                .join(","),
            row[3]
        );
        assert_eq!(format!("{calls:?}"), row[4]);
        let mut random = LegacyRandomSource::new(row[2].parse().unwrap());
        let picked = if selected.is_empty() {
            "-"
        } else {
            selected[random.next_int(selected.len() as i32) as usize].path()
        };
        assert_eq!(picked, row[5]);
        assert_eq!(random.next_long(), row[6].parse::<i64>().unwrap());
        count += 1;
    }
    assert_eq!(count, 30);
}

#[test]
fn moon_condition_is_decoded_but_required_only_when_its_priority_is_evaluated() {
    let condition = SpawnCondition::decode(
        &serde_json::json!({"type":"moon_brightness","range":{"min":0.5,"max":1.0}}),
        &resources(),
    )
    .unwrap();
    let SpawnCondition::MoonBrightness(range) = &condition else {
        panic!()
    };
    assert_eq!(
        *range,
        DoubleBounds {
            min: Some(0.5),
            max: Some(1.0)
        }
    );
    let level = crate::entity::tests::persistence_level();
    let error = condition
        .test(BlockPos::ZERO, &"minecraft:plains".parse().unwrap(), &level)
        .unwrap_err();
    assert_eq!(
        error,
        LevelError::UnsupportedSpawnCondition("minecraft:moon_brightness".parse().unwrap())
    );
    let registry = SpawnVariantRegistry {
        entries: vec![
            SpawnVariant {
                key: "test:winner".parse().unwrap(),
                selectors: vec![SpawnSelector {
                    priority: 1,
                    condition: None,
                }],
            },
            SpawnVariant {
                key: "test:moon".parse().unwrap(),
                selectors: vec![SpawnSelector {
                    priority: 0,
                    condition: Some(condition),
                }],
            },
        ],
    };
    assert_eq!(
        registry.select(|_| Err::<bool, _>(error.clone())).unwrap(),
        [&registry.entries[0].key]
    );
    assert!(
        SpawnCondition::decode(
            &serde_json::json!({"type":"biome","biomes":"#test:absent"}),
            &resources()
        )
        .is_err()
    );
}
