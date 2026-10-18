use super::*;
use crate::{
    region::{ChunkStatus, RegionChunk},
    structure::{
        BoundingBox,
        processor::tests::{StaticBlocks, generator},
        template::{PlacementRandom, StructureTemplate},
    },
};
use ferraloom_util::{Mirror, Rotation};
use std::io::Read;
#[test]
fn capped_finalization_shuffle_changed_nbt_clipping_and_settings_rng_match_vanilla() {
    let (resources, generator) = generator();
    let tags = BlockTags::load(&resources).unwrap();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/capped_processors.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: Value = serde_json::from_str(&text).unwrap();
    let blocks = (0..21)
        .map(|i| {
            let mut nbt = ferraloom_nbt::parse_compound("{keep:3}").unwrap();
            if i % 4 == 0 {
                nbt.insert("touched", ferraloom_nbt::Tag::Int(1));
            }
            StructureBlockInfo {
                position: [i % 7 - 3, i % 3 - 1, i / 7 - 1],
                state: if i % 5 == 0 {
                    Block::AIR
                } else if i % 3 == 0 {
                    Block::DIRT
                } else {
                    Block::STONE
                }
                .default_state(),
                nbt: Some(nbt),
            }
        })
        .collect::<Vec<_>>();
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        let seed = case["seed"].as_i64().unwrap();
        let mut input = blocks.clone();
        if case["sparse_nbt"].as_bool().unwrap() {
            for (i, block) in input.iter_mut().enumerate() {
                if i % 2 != 0 {
                    block.nbt = None;
                }
            }
        }
        let mut chunks = Vec::new();
        for z in -1..=1 {
            for x in -1..=1 {
                let mut chunk = generator.new_chunk([x, z]);
                for z in 0..16 {
                    for x in 0..16 {
                        for y in 60..=74 {
                            chunk.set_block(x, y, z, Block::STONE.default_state());
                        }
                    }
                }
                chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
            }
        }
        let region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Terrain; 2],
            0,
            None,
            &StaticBlocks,
            seed,
            63,
        )
        .unwrap();
        let rng = case["rng"].as_u64().unwrap();
        let mut shared =
            WorldgenRandom::new(ferraloom_util::random::Algorithm::Legacy.new_instance(1234));
        let mut settings = StructurePlaceSettings {
            rotation: Rotation::VALUES[case["rotation"].as_u64().unwrap() as usize],
            mirror: [Mirror::None, Mirror::LeftRight, Mirror::FrontBack]
                [case["mirror"].as_u64().unwrap() as usize],
            pivot: [1, 0, 2],
            bounding_box: Some(BoundingBox::new([0, 63, 0], [15, 70, 15])),
            random: match rng {
                0 => PlacementRandom::Positional,
                1 => PlacementRandom::Shared,
                _ => PlacementRandom::Fixed(
                    ferraloom_util::random::Algorithm::Legacy.new_instance(999),
                ),
            },
            ..Default::default()
        };
        let processors = StructureProcessor::decode_list(&case["definition"]).unwrap();
        let processed = StructureTemplate::default()
            .process_block_infos(
                &region,
                [0, 64, 0],
                [7, 65, -3],
                &mut settings,
                &processors,
                &tags,
                &input,
                &mut shared,
            )
            .unwrap();
        let expected = case["blocks"].as_array().unwrap();
        assert_eq!(processed.len(), expected.len(), "case={index}");
        for (i, (block, expected)) in processed.iter().zip(expected).enumerate() {
            assert_eq!(
                block.position,
                std::array::from_fn(|i| expected["pos"][i].as_i64().unwrap() as i32),
                "case={index} block={i}"
            );
            assert_eq!(
                block.state.id(),
                expected["state"].as_u64().unwrap() as u16,
                "case={index} block={i}"
            );
            assert_eq!(
                block.nbt,
                expected
                    .get("nbt")
                    .map(|v| ferraloom_nbt::parse_compound(v.as_str().unwrap()).unwrap()),
                "case={index} block={i}"
            );
        }
        assert_eq!(
            shared.next_int(),
            case["next"].as_i64().unwrap() as i32,
            "case={index}"
        );
        if rng == 2 {
            assert_eq!(
                settings.with_random([0; 3], &mut shared, |r| r.next_int()),
                case["fixed_next"].as_i64().unwrap() as i32,
                "case={index}"
            );
        }
    }
}
