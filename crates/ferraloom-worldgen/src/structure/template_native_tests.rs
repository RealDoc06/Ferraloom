//! Exhaustive real-jar comparison. Vectors stay transient and only one template
//! and placement case are retained at a time.
use super::*;
use crate::{
    Error,
    feature::LiveWorldGenLevel,
    structure::{processor::StructureProcessor, rule::BlockTags},
};
use ferraloom_data::BlockState;
use ferraloom_util::random::{Algorithm, LegacyRandomSource, Random, RandomSource, WorldgenRandom};
use serde_json::Value;
use std::{collections::BTreeSet, io::BufRead, path::PathBuf, process::Command};

#[derive(Default)]
struct PlacementWrites(Vec<([i32; 3], u16, u32)>);
impl LiveWorldGenLevel for PlacementWrites {
    fn min_y(&self) -> i32 {
        -64
    }
    fn height(&self) -> i32 {
        384
    }
    fn seed(&self) -> i64 {
        0
    }
    fn sea_level(&self) -> i32 {
        63
    }
    fn set_block(&mut self, p: [i32; 3], state: BlockState, flags: u32) -> Result<bool, Error> {
        self.0.push((p, state.id(), flags));
        Ok(true)
    }
    fn has_block_entity_at(&self, _: [i32; 3]) -> Result<bool, Error> {
        Ok(false)
    }
    fn mark_template_block_entity_changed(&mut self, _: [i32; 3]) -> Result<(), Error> {
        Ok(())
    }
}
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap().try_into().unwrap())
}
fn nbt(v: &Value) -> Option<CompoundTag> {
    v.as_str()
        .map(|s| ferraloom_nbt::parse_compound(s).unwrap())
}
struct TemporaryVectors(PathBuf);
impl Drop for TemporaryVectors {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
#[ignore = "runs exhaustive Java template loading and placement against the official jar"]
fn every_vanilla_template_load_and_transformed_block_placement_matches_native() {
    let temporary = TemporaryVectors(std::env::temp_dir().join(format!(
        "ferraloom-native-templates-{}.jsonl.gz",
        std::process::id()
    )));
    let vectors = std::env::var_os("FERRALOOM_STRUCTURE_TEMPLATE_VECTORS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            assert!(
                Command::new("python3")
                    .arg(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/java/structures/native_templates.py"
                    ))
                    .arg(&temporary.0)
                    .status()
                    .unwrap()
                    .success()
            );
            temporary.0.clone()
        });
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides jar");
    let resources = ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
    ]);
    let tags = BlockTags::load(&resources).unwrap();
    let input = std::io::BufReader::new(flate2::read::GzDecoder::new(
        std::fs::File::open(vectors).unwrap(),
    ));
    let mut lines = input.lines();
    let mut seen = BTreeSet::new();
    let mut placements = 0;
    while let Some(line) = lines.next() {
        let case: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let id: Identifier = format!("minecraft:{}", case["id"].as_str().unwrap())
            .parse()
            .unwrap();
        assert!(seen.insert(id.clone()), "duplicate template {id}");
        let template = StructureTemplate::load(&resources, &id).unwrap();
        assert_eq!(template.size, point(&case["size"]), "{id} size");
        let palettes = case["palettes"].as_array().unwrap();
        assert_eq!(
            template.palettes.len(),
            palettes.len(),
            "{id} palette count"
        );
        for (palette, expected) in template.palettes.iter().zip(palettes) {
            let expected = expected.as_array().unwrap();
            assert_eq!(palette.blocks.len(), expected.len(), "{id} block count");
            for (index, (actual, expected)) in palette.blocks.iter().zip(expected).enumerate() {
                assert_eq!(
                    actual.position,
                    point(expected),
                    "{id} block {index} position"
                );
                assert_eq!(
                    u64::from(actual.state.id()),
                    expected[3].as_u64().unwrap(),
                    "{id} block {index} state"
                );
                assert_eq!(actual.nbt, nbt(&expected[4]), "{id} block {index} NBT");
            }
        }
        let entities = case["entities"].as_array().unwrap();
        assert_eq!(template.entities.len(), entities.len(), "{id} entity count");
        for (actual, expected) in template.entities.iter().zip(entities) {
            assert_eq!(
                actual.block_position,
                point(&expected["block_pos"]),
                "{id} entity block position"
            );
            for axis in 0..3 {
                assert_eq!(
                    actual.position[axis].to_bits(),
                    expected["pos"][axis]
                        .as_str()
                        .unwrap()
                        .parse::<i64>()
                        .unwrap() as u64,
                    "{id} entity position"
                );
            }
            assert_eq!(
                Some(&actual.nbt),
                nbt(&expected["nbt"]).as_ref(),
                "{id} entity NBT"
            );
        }
        // Release the full native loaded template before reading placement output.
        drop(case);
        for scenario in 0..12 {
            let expected: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
            assert_eq!(expected["scenario"], scenario);
            let mut settings = StructurePlaceSettings {
                mirror: [Mirror::None, Mirror::LeftRight, Mirror::FrontBack][scenario as usize % 3],
                rotation: Rotation::VALUES[scenario as usize % 4],
                pivot: [1, 0, -2],
                known_shape: true,
                ignore_entities: true,
                liquid_settings: LiquidSettings::IgnoreWaterlogging,
                ..Default::default()
            };
            if scenario % 2 == 1 {
                settings.bounding_box = Some(BoundingBox::new([-16, 63, -16], [16, 90, 16]));
            }
            let processors = match scenario % 3 {
                1 => {
                    settings.random =
                        PlacementRandom::Fixed(Random::Legacy(LegacyRandomSource::new(91)));
                    vec![StructureProcessor::BlockIgnore(vec![
                        Block::AIR,
                        Block::STRUCTURE_BLOCK,
                    ])]
                }
                2 => {
                    settings.random = PlacementRandom::Shared;
                    vec![
                        StructureProcessor::BlackstoneReplace,
                        StructureProcessor::BlockRot {
                            integrity: 0.8,
                            rottable: None,
                        },
                    ]
                }
                _ => Vec::new(),
            };
            let mut random = WorldgenRandom::new(Algorithm::Xoroshiro.new_instance(17));
            let mut writes = PlacementWrites::default();
            let result = template
                .place_in_world(
                    &mut writes,
                    [7, 64, 7],
                    [3, 65, 4],
                    &mut settings,
                    &processors,
                    &tags,
                    &mut random,
                    18,
                )
                .unwrap();
            assert_eq!(
                result,
                expected["result"].as_bool().unwrap(),
                "{id} scenario {scenario}"
            );
            let expected_writes = expected["writes"].as_array().unwrap();
            assert_eq!(
                writes.0.len(),
                expected_writes.len(),
                "{id} scenario {scenario} writes"
            );
            for (index, (actual, expected)) in writes.0.iter().zip(expected_writes).enumerate() {
                assert_eq!(
                    *actual,
                    (
                        point(expected),
                        expected[3].as_u64().unwrap() as u16,
                        expected[4].as_u64().unwrap() as u32
                    ),
                    "{id} scenario {scenario} write {index}"
                );
            }
            assert_eq!(
                random.next_int(),
                expected["next"].as_i64().unwrap() as i32,
                "{id} scenario {scenario} RNG"
            );
            let settings_next = match &mut settings.random {
                PlacementRandom::Fixed(random) => random.next_int(),
                _ => LegacyRandomSource::new(91).next_int(),
            };
            assert_eq!(
                settings_next,
                expected["settings_next"].as_i64().unwrap() as i32,
                "{id} scenario {scenario} settings RNG"
            );
            placements += 1;
        }
    }
    let expected_ids = resources
        .list("structure/", ".nbt")
        .into_iter()
        .map(|id| {
            id.with_path(
                id.path()
                    .strip_prefix("structure/")
                    .unwrap()
                    .strip_suffix(".nbt")
                    .unwrap(),
            )
            .unwrap()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(seen, expected_ids, "native dump must cover every template");
    assert_eq!(placements, seen.len() * 12);
    eprintln!(
        "{} templates, {placements} exact ordered placement comparisons",
        seen.len()
    );
}
