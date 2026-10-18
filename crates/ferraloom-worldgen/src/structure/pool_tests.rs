use super::*;
use crate::structure::{
    alias::{PoolAliasBinding, PoolAliasLookup},
    processor::tests::generator,
};
use ferraloom_util::random::LegacyRandomSource;
use std::io::Read;
fn golden(bytes: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn name(element: &StructurePoolElement) -> String {
    match element {
        StructurePoolElement::Empty => "empty".into(),
        StructurePoolElement::Single {
            location, legacy, ..
        } => format!("{}:{location}", if *legacy { "legacy" } else { "single" }),
        StructurePoolElement::Feature { .. } => "feature".into(),
        StructurePoolElement::List { elements, .. } => format!(
            "[{}]",
            elements
                .iter()
                .map(|e| name(e))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}
#[test]
fn every_vanilla_pool_selection_shuffle_and_following_rng_matches() {
    let (resources, _) = generator();
    let pools = StructurePools::load(&resources, &mut |_| {
        Err(Error("test does not load inline features".into()))
    })
    .unwrap();
    for case in golden(include_bytes!("testdata/pools.json.gz"))
        .as_array()
        .unwrap()
    {
        let pool = pools
            .get(&case["pool"].as_str().unwrap().parse().unwrap())
            .unwrap();
        let mut random = LegacyRandomSource::new(case["seed"].as_i64().unwrap());
        for sample in case["samples"].as_array().unwrap() {
            assert_eq!(
                name(&pool.random_template(&mut random)),
                sample.as_str().unwrap(),
                "{}",
                case["pool"]
            );
        }
        let actual = pool
            .shuffled_templates(&mut random)
            .iter()
            .map(|e| name(e))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            case["shuffled"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{}",
            case["pool"]
        );
        assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
    }
}
#[test]
fn nested_pool_aliases_match_vanilla_positional_streams() {
    for case in golden(include_bytes!("testdata/aliases.json.gz"))
        .as_array()
        .unwrap()
    {
        let bindings = case["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(PoolAliasBinding::decode)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let position = std::array::from_fn(|i| case["pos"][i].as_i64().unwrap() as i32);
        let lookup =
            PoolAliasLookup::create(&bindings, position, case["seed"].as_i64().unwrap()).unwrap();
        for (alias, expected) in case["values"].as_object().unwrap() {
            assert_eq!(
                lookup.lookup(&alias.parse().unwrap()).to_string(),
                expected.as_str().unwrap()
            );
        }
    }
}
