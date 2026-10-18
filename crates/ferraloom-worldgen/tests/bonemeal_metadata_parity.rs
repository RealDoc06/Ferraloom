use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_worldgen::{
    Identifier,
    feature::{Feature, FeatureLoader, FeatureRegistry},
};
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::Arc};

fn golden() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("golden/bonemeal_metadata.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn vanilla() -> Pack {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    Pack::vanilla_jar(Path::new(&jar)).unwrap()
}
fn name(registry: &FeatureRegistry, feature: &Arc<dyn Feature>) -> Option<String> {
    registry
        .features
        .iter()
        .find(|(_, candidate)| Arc::ptr_eq(candidate, feature))
        .map(|(id, _)| id.to_string())
}
#[test]
fn biome_bone_meal_lists_match_vanilla() {
    let resources = ResourceManager::new(vec![vanilla()]);
    let registry = FeatureLoader::new(&resources).unwrap().load_all().unwrap();
    let cases = golden();
    assert_eq!(
        registry.biome_bone_meal_features.len(),
        cases["biomes"].as_object().unwrap().len()
    );
    for (biome, expected) in cases["biomes"].as_object().unwrap() {
        let actual = registry
            .bone_meal_features(&biome.parse().unwrap())
            .unwrap()
            .iter()
            .map(|feature| name(&registry, feature).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(json!(actual), *expected, "{biome}");
    }
    assert!(
        registry
            .bone_meal_features(&"ferraloom:missing".parse().unwrap())
            .is_err()
    );
}
#[test]
fn composite_holder_traversal_and_duplicate_bone_meal_weight_match_vanilla() {
    let cases = golden();
    let mut pack = Pack::new("bone-meal metadata cases");
    let parent: Identifier = "ferraloom:parent".parse().unwrap();
    pack.insert(
        "ferraloom:worldgen/feature/parent.json".parse().unwrap(),
        serde_json::to_vec(&cases["composites"][6]["config"]["feature"]).unwrap(),
    );
    pack.insert(
        "ferraloom:worldgen/placed_feature/parent.json"
            .parse()
            .unwrap(),
        serde_json::to_vec(&json!({"feature":parent.to_string(), "placement":[]})).unwrap(),
    );
    pack.insert(
        "ferraloom:worldgen/biome/metadata.json".parse().unwrap(),
        serde_json::to_vec(&json!({"features":[["ferraloom:parent"],["ferraloom:parent"]]}))
            .unwrap(),
    );
    pack.insert(
        "minecraft:tags/worldgen/feature/can_spawn_from_bone_meal.json"
            .parse()
            .unwrap(),
        serde_json::to_vec(&json!({"values":[parent.to_string()]})).unwrap(),
    );
    let resources = ResourceManager::new(vec![vanilla(), pack]);
    let mut loader = FeatureLoader::new(&resources).unwrap();
    loader.load_biome_features().unwrap();
    loader.load_bone_meal_features().unwrap();
    let placed = cases["composites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| loader.placed(&case["config"]).unwrap())
        .collect::<Vec<_>>();
    let registry = loader.finish();
    for (case, placed) in cases["composites"].as_array().unwrap().iter().zip(placed) {
        let mut actual = Vec::new();
        placed.visit_features(&mut |feature| actual.push(name(&registry, feature)));
        assert_eq!(json!(actual), case["traversal"]);
    }
    let actual = registry
        .bone_meal_features(&"ferraloom:metadata".parse().unwrap())
        .unwrap()
        .iter()
        .map(|feature| name(&registry, feature).unwrap())
        .collect::<Vec<_>>();
    let mut expected = vec![parent.to_string()];
    expected.extend(
        cases["composites"][6]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned()),
    );
    expected.extend(expected.clone());
    assert_eq!(actual, expected);
}

#[test]
fn actual_huge_mushroom_type_and_radius_match_vanilla() {
    let resources = ResourceManager::new(vec![vanilla()]);
    let mut loader = FeatureLoader::new(&resources).unwrap();
    for case in golden()["mushrooms"].as_array().unwrap() {
        let actual = loader
            .feature(&case["config"])
            .unwrap()
            .huge_mushroom_foliage_radius();
        assert_eq!(json!(actual), case["radius"]);
    }
}
