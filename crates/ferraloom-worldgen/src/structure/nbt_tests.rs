use super::*;
use crate::{
    feature::{Feature, FeatureContext, PlacedFeature},
    structure::{pool::StructurePoolElement, processor::tests::generator},
};
use serde_json::Value;
use std::{io::Read, sync::Arc};
#[derive(Debug)]
struct EncodeOnly;
impl Feature for EncodeOnly {
    fn place(&self, _: &mut FeatureContext<'_, '_>) -> Result<bool, crate::Error> {
        unreachable!("codec-only fixture")
    }
}
fn inline(_: &Value) -> Result<Arc<PlacedFeature>, crate::Error> {
    Ok(Arc::new(PlacedFeature {
        id: None,
        feature: Arc::new(EncodeOnly),
        placement: Vec::new(),
    }))
}
#[test]
fn typed_processor_rule_and_named_pool_nbt_matches_vanilla() {
    let (resources, _) = generator();
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("testdata/structure_codecs.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    for case in serde_json::from_str::<Value>(&text)
        .unwrap()
        .as_array()
        .unwrap()
    {
        let value = &case["input"];
        let actual = match case["kind"].as_str().unwrap() {
            "processor" => StructureProcessor::decode(value).unwrap().to_nbt(),
            "rule" => RuleTest::decode(value).unwrap().to_nbt(),
            "position" => PosRuleTest::decode(value).unwrap().to_nbt(),
            "modifier" => RuleBlockEntityModifier::decode(value).unwrap().to_nbt(),
            "pool" => StructurePoolElement::decode(value, &resources, &mut inline)
                .unwrap()
                .to_nbt()
                .unwrap(),
            _ => unreachable!(),
        };
        let expected = ferraloom_nbt::parse(case["encoded"].as_str().unwrap()).unwrap();
        if case["exact"].as_bool().unwrap() {
            assert_eq!(actual, expected, "{case}");
        } else {
            // Known gap: inline feature codec re-encoding removes unused config
            // fields. Retaining source JSON cannot yet promise canonical NBT.
            assert_eq!(
                actual,
                crate::structure::processor::json_nbt(value).unwrap()
            );
            assert_ne!(
                actual, expected,
                "authored inline default-field gap changed"
            );
        }
    }
}
