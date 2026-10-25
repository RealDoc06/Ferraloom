use super::*;
use crate::{block::UnportedBehavior, entity::damage::DamageTypeKey};
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::VarInt,
    component::{DamageTypeSet, DataComponentPatch, Unit},
};
const VECTORS: &str = include_str!("../../../tests/java/entity/vectors.tsv");
fn stack(item: Item, count: i32, mode: &str) -> ItemStack {
    let mut patch = DataComponentPatch::default();
    match mode {
        "remove-damage" => patch.remove(ComponentType::DAMAGE),
        "force-damage" => {
            patch.set(DataComponent::MaxDamage(VarInt(17)));
            patch.set(DataComponent::Damage(VarInt(0)));
        }
        "unbreakable" => patch.set(DataComponent::Unbreakable(Unit)),
        "prototype" => {}
        _ => panic!("native component edit"),
    }
    let mut stack = ItemStack::new(item, 1, patch);
    stack.0.as_mut().unwrap().count = count;
    stack
}
#[test]
fn native_damageable_query_uses_all_three_effective_components_and_hides_empty_payloads() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("item-damageable\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let stack = stack(Item::from_name(f[1]).unwrap(), f[3].parse().unwrap(), f[2]);
        assert_eq!(is_damageable_item(&stack).to_string(), f[4], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 48);
}
fn resistance(mode: &str, count: i32) -> ItemStack {
    let mut patch = DataComponentPatch::default();
    let set = match mode {
        "none" | "prototype" => None,
        "direct-fire" => Some(HolderSet::Direct(vec![
            "minecraft:in_fire".parse().unwrap(),
        ])),
        "direct-mixed" => Some(HolderSet::Direct(vec![
            "minecraft:in_fire".parse().unwrap(),
            "minecraft:fall".parse().unwrap(),
        ])),
        "direct-empty" => Some(HolderSet::Direct(vec![])),
        "tag-fire" => Some(HolderSet::Named("minecraft:is_fire".parse().unwrap())),
        "tag-fall" => Some(HolderSet::Named("minecraft:is_fall".parse().unwrap())),
        "missing" => Some(HolderSet::Named("test:missing".parse().unwrap())),
        _ => panic!("native resistance set"),
    };
    if let Some(set) = set {
        patch.set(DataComponent::DamageResistant(DamageTypeSet(set)));
    }
    let mut stack = ItemStack::new(
        if mode == "prototype" {
            Item::NETHERITE_INGOT
        } else {
            Item::STONE
        },
        1,
        patch,
    );
    stack.0.as_mut().unwrap().count = count;
    stack
}
#[test]
fn native_resistance_queries_match_prototypes_direct_sets_tags_and_empty_stack_components() {
    let level = crate::entity::tests::persistence_level();
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("item-damage-resistant\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let key = DamageTypeKey::ALL
            .iter()
            .copied()
            .find(|key| key.name() == f[3])
            .unwrap();
        let source = level.damage_sources().source(key);
        let stack = resistance(f[1], f[2].parse().unwrap());
        assert_eq!(
            can_be_hurt_by(&stack, &source, &level).unwrap().to_string(),
            f[4],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 105);
}
#[test]
fn a_required_named_damage_tag_never_uses_warn_fallback_and_empty_payloads_do_not_query_it() {
    let mut level = crate::entity::tests::persistence_level();
    let source = level.damage_sources().in_fire();
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert_eq!(
            can_be_hurt_by(&resistance("missing", 1), &source, &level),
            Err(LevelError::MissingDamageTypeTag)
        );
        for count in [-1, 0] {
            assert!(can_be_hurt_by(&resistance("missing", count), &source, &level).unwrap());
        }
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
