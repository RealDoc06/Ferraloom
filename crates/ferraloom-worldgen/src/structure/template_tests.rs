use super::*;
use serde_json::Value;
use std::{io::Read, path::Path};
fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn point(v: &Value) -> [i32; 3] {
    std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
}
#[test]
fn every_vanilla_template_loads_with_matching_palette_order_and_entities() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides jar");
    let resources = ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(Path::new(&jar)).unwrap(),
    ]);
    for case in golden(include_bytes!("testdata/templates.json.gz"))
        .as_array()
        .unwrap()
    {
        let id: Identifier = case["id"].as_str().unwrap().parse().unwrap();
        let template = StructureTemplate::load(&resources, &id).unwrap();
        assert_eq!(template.size, point(&case["size"]), "{id} size");
        assert_eq!(
            template.entities.len(),
            case["entities"].as_u64().unwrap() as usize,
            "{id} entities"
        );
        assert_eq!(
            template.palettes.len(),
            case["palettes"].as_array().unwrap().len(),
            "{id} palettes"
        );
        for (palette, expected) in template
            .palettes
            .iter()
            .zip(case["palettes"].as_array().unwrap())
        {
            assert_eq!(
                palette.blocks.len(),
                expected["size"].as_u64().unwrap() as usize,
                "{id} blocks"
            );
            assert_eq!(
                palette.blocks.iter().filter(|b| b.nbt.is_some()).count(),
                expected["nbt_count"].as_u64().unwrap() as usize,
                "{id} nbt blocks"
            );
            for sample in expected["samples"].as_array().unwrap() {
                let block = &palette.blocks[sample["index"].as_u64().unwrap() as usize];
                assert_eq!(
                    block.position,
                    point(&sample["pos"]),
                    "{id} position {sample}"
                );
                assert_eq!(
                    block.state.id(),
                    sample["state"].as_u64().unwrap() as u16,
                    "{id} state {sample}"
                );
                assert_eq!(
                    block.nbt.is_some(),
                    sample["nbt"].as_bool().unwrap(),
                    "{id} nbt {sample}"
                );
            }
        }
    }
}
#[test]
fn integer_and_entity_geometry_matches_vanilla_including_overflow() {
    for case in golden(include_bytes!("testdata/template_geometry.json.gz"))
        .as_array()
        .unwrap()
    {
        let mirror = [Mirror::None, Mirror::LeftRight, Mirror::FrontBack]
            [case["mirror"].as_u64().unwrap() as usize];
        let rotation = Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize];
        let pivot = point(&case["pivot"]);
        let p = point(&case["pos"]);
        assert_eq!(
            transform_block(p, mirror, rotation, pivot),
            point(&case["block"]),
            "{case}"
        );
        let p = [
            f64::from(p[0]) + 0.25,
            f64::from(p[1]) + 0.5,
            f64::from(p[2]) + 0.75,
        ];
        let actual = transform_entity(p, mirror, rotation, pivot);
        for (axis, coordinate) in actual.iter().enumerate() {
            assert_eq!(
                coordinate.to_bits(),
                case["entity"][axis]
                    .as_str()
                    .unwrap()
                    .parse::<i64>()
                    .unwrap() as u64,
                "{case}"
            );
        }
    }
}

#[test]
fn clipped_mult_palette_template_block_placement_matches_vanilla() {
    use crate::{
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::{
            processor::{
                StructureProcessor,
                tests::{StaticBlocks, generator},
            },
            rule::BlockTags,
        },
    };
    use ferraloom_nbt::ListTag;
    use ferraloom_util::random::{
        Algorithm, LegacyRandomSource, Random, RandomSource, WorldgenRandom,
    };
    use std::collections::BTreeMap;
    let (resources, generator) = generator();
    let tags = BlockTags::load(&resources).unwrap();
    let mut tag = ferraloom_nbt::parse_compound("{DataVersion:5023,size:[4,3,3]}").unwrap();
    let mut palettes = Vec::new();
    for variant in [false, true] {
        let states = [
            Block::STONE,
            Block::OAK_STAIRS,
            Block::STONE_BRICK_SLAB,
            Block::OAK_LOG,
            Block::AIR,
            if variant {
                Block::MOSSY_COBBLESTONE
            } else {
                Block::STONE_BRICKS
            },
        ];
        let values = states
            .into_iter()
            .map(|block| {
                let mut tag = CompoundTag::new();
                tag.insert("id", Tag::String(block.name().into()));
                Tag::Compound(tag)
            })
            .collect();
        palettes.push(Tag::List(ListTag(values)));
    }
    tag.insert("palettes", Tag::List(ListTag(palettes)));
    let mut blocks = Vec::new();
    for x in 0..4 {
        for y in 0..3 {
            for z in 0..3 {
                let mut block = CompoundTag::new();
                block.insert(
                    "pos",
                    Tag::List(ListTag(vec![Tag::Int(x), Tag::Int(y), Tag::Int(z)])),
                );
                block.insert("state", Tag::Int((x * 3 + y + z) % 6));
                blocks.push(Tag::Compound(block));
            }
        }
    }
    tag.insert("blocks", Tag::List(ListTag(blocks)));
    let template = StructureTemplate::from_nbt(&tag);
    for case in golden(include_bytes!("testdata/placed_templates.json.gz"))
        .as_array()
        .unwrap()
    {
        let mut chunks = [RegionChunk::new(
            generator.new_chunk([0, 0]),
            ChunkStatus::Terrain,
        )];
        let mirror = [Mirror::None, Mirror::LeftRight, Mirror::FrontBack]
            [case["mirror"].as_u64().unwrap() as usize];
        let rotation = Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize];
        let mut settings = StructurePlaceSettings {
            mirror,
            rotation,
            pivot: [1, 0, -2],
            bounding_box: Some(BoundingBox::new([2, 63, 2], [13, 69, 13])),
            known_shape: true,
            ignore_entities: true,
            liquid_settings: LiquidSettings::IgnoreWaterlogging,
            ..Default::default()
        };
        if case["external"].as_bool().unwrap() {
            settings.random = Some(Random::Legacy(LegacyRandomSource::new(91)));
        }
        let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
        let result = {
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
                0,
                63,
            )
            .unwrap();
            template
                .place_in_world(
                    &mut region,
                    [7, 64, 7],
                    [3, 65, 4],
                    &mut settings,
                    &[
                        StructureProcessor::BlockIgnore(vec![Block::AIR]),
                        StructureProcessor::BlackstoneReplace,
                    ],
                    &tags,
                    &mut random,
                    18,
                )
                .unwrap()
        };
        assert_eq!(result, case["result"].as_bool().unwrap());
        let placed = chunks[0]
            .poi_changes
            .iter()
            .map(|&(p, _, state)| (p, state.id()))
            .collect::<BTreeMap<_, _>>();
        let expected = case["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (point(v), v[3].as_u64().unwrap() as u16))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            placed, expected,
            "{mirror:?} {rotation:?} external={}",
            case["external"]
        );
        assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
        assert_eq!(
            settings.random.as_mut().map_or_else(
                || LegacyRandomSource::new(91).next_int(),
                RandomSource::next_int
            ),
            case["settings_next"].as_i64().unwrap() as i32
        );
    }
}
