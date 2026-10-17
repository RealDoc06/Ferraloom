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
