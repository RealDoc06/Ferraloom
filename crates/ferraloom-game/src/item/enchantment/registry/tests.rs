use super::*;
use crate::level::{LevelContext, LevelError};
use ferraloom_data::datapack::{Pack, ResourceManager};

#[test]
fn native_registry_effect_presence_and_protocol_order_match_the_merged_resources() {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla datapack");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    let registry = EnchantmentRegistry::load(&resources).unwrap();
    let actual: Vec<_> = registry
        .iter()
        .enumerate()
        .map(|(id, (key, enchantment))| {
            assert_eq!(registry.id(key), Some(id as i32));
            assert!(std::ptr::eq(
                registry.by_id(id as i32).unwrap(),
                enchantment
            ));
            let mut components: Vec<_> = enchantment
                .effect_components()
                .map(|id| id.name())
                .collect();
            components.sort_unstable();
            format!("enchantment-registry\t{key}\t{}", components.join(","))
        })
        .collect();
    let expected: Vec<_> = include_str!("../../../../tests/java/items/enchantments/registry.tsv")
        .lines()
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(actual, expected);
    let protocol_keys: Vec<_> = resources
        .registry("enchantment")
        .into_keys()
        .map(|key| key.to_string())
        .collect();
    assert_eq!(
        registry
            .iter()
            .map(|(key, _)| key.to_string())
            .collect::<Vec<_>>(),
        protocol_keys
    );
}

#[test]
fn enchantment_registry_access_is_required_and_fixtures_install_real_values() {
    let context = LevelContext::fixture(0, 63);
    assert!(
        context
            .enchantments()
            .unwrap()
            .get(&"minecraft:mending".parse().unwrap())
            .unwrap()
            .has_effect(EffectComponent::REPAIR_WITH_XP)
    );
    let bare = LevelContext::new(
        context.seed,
        context.sea_level,
        context.generator.clone(),
        context.features.clone(),
        context.structures.clone(),
        context.damage_types().clone(),
        context.initial_difficulty,
    );
    assert!(matches!(
        bare.enchantments(),
        Err(LevelError::MissingEnchantments)
    ));
}
