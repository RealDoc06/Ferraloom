use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{
        EntityState,
        animal::Cow,
        item::PrimedTnt,
        player::{GameType, ServerPlayer, inventory},
    },
};
use ferraloom_data::{EntityType, Item};
use ferraloom_protocol::{
    codec::VarInt,
    component::{Enchantment, EquipmentSlot, ItemEnchantments, ItemStack},
};
use ferraloom_util::{Uuid, Vec3};

fn float(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn player(level: &mut Level) -> EntityRef {
    let entity = level.next_entity_ref();
    let state = ServerPlayer::new(
        Uuid::new(0, entity.serial() as i64),
        "combat".into(),
        level.next_entity_id().unwrap(),
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(state)))
        .unwrap();
    entity
}

#[test]
fn native_combat_rules_preserve_float_armor_toughness_magic_and_empty_weapon_steps() {
    let mut level = crate::entity::tests::level(false);
    let victim = player(&mut level);
    let attacker = player(&mut level);
    let mut armor_cases = 0;
    let mut magic_cases = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        match fields[0] {
            "combat-armor" => {
                level
                    .entity_mut::<ServerPlayer>(attacker)
                    .unwrap()
                    .inventory
                    .set_selected_item(if fields[1] == "stone" {
                        ItemStack::new(Item::STONE, 1, Default::default())
                    } else {
                        ItemStack::default()
                    });
                let source = if fields[1] == "absent" {
                    level.damage_sources().generic()
                } else {
                    level.damage_sources().player_attack(attacker)
                };
                let damage = get_damage_after_absorb(
                    victim,
                    float(fields[2]),
                    &source,
                    float(fields[3]),
                    float(fields[4]),
                    &mut level,
                )
                .unwrap();
                assert_eq!(damage.to_bits(), float(fields[5]).to_bits(), "{row}");
                armor_cases += 1;
            }
            "combat-magic" => {
                assert_eq!(
                    get_damage_after_magic_absorb(float(fields[1]), float(fields[2])).to_bits(),
                    float(fields[3]).to_bits(),
                    "{row}",
                );
                magic_cases += 1;
            }
            _ => {}
        }
    }
    assert_eq!((armor_cases, magic_cases), (36, 12));
}

#[test]
fn native_damage_weapon_uses_the_direct_entity_and_live_selected_stack() {
    let mut level = crate::entity::tests::level(false);
    let attacker = player(&mut level);
    let cow = level.next_entity_ref();
    let mut state = Cow::with_parent(level.create_entity_data(EntityType::COW).unwrap());
    state.as_living_mut().equipment.set(
        EquipmentSlot::Mainhand,
        ItemStack::new(Item::IRON_AXE, 1, Default::default()),
    );
    level
        .add_entity(cow, EntityState::Cow(Box::new(state)))
        .unwrap();
    let tnt = level.next_entity_ref();
    let state = PrimedTnt::new(level.next_entity_id().unwrap(), 123);
    level
        .add_entity(tnt, EntityState::PrimedTnt(Box::new(state)))
        .unwrap();
    let holder = level.damage_sources().generic().type_holder().clone();
    let mut actual = Vec::new();
    let mut read = |name: &str, source: &DamageSource, level: &Level| {
        actual.push(format!(
            "damage-weapon\t{name}\t{}",
            source
                .weapon_item(level)
                .unwrap()
                .map(|stack| stack.item().name())
                .unwrap_or("null"),
        ));
    };
    read("absent", &DamageSource::new(holder.clone()), &level);
    {
        let player = level.entity_mut::<ServerPlayer>(attacker).unwrap();
        player.inventory.set_selected_slot(0);
        player
            .inventory
            .set_selected_item(ItemStack::new(Item::STONE, 1, Default::default()));
        player.inventory.set_selected_slot(1);
        player
            .inventory
            .set_selected_item(ItemStack::new(Item::GOLD_INGOT, 1, Default::default()));
        player.inventory.set_selected_slot(0);
    }
    let source = DamageSource::with_entities(holder.clone(), Some(attacker), Some(cow));
    read("selected0", &source, &level);
    assert!(std::ptr::eq(
        source.weapon_item(&level).unwrap().unwrap(),
        level
            .entity::<ServerPlayer>(attacker)
            .unwrap()
            .get_main_hand_item(),
    ));
    level
        .entity_mut::<ServerPlayer>(attacker)
        .unwrap()
        .inventory
        .set_selected_slot(1);
    read("selected1", &source, &level);
    inventory::set_count(
        level
            .entity_mut::<ServerPlayer>(attacker)
            .unwrap()
            .inventory
            .selected_item_mut(),
        0,
    );
    read("retained-empty", &source, &level);
    read(
        "cow-direct",
        &DamageSource::with_entities(holder.clone(), Some(cow), Some(attacker)),
        &level,
    );
    read(
        "tnt-direct",
        &DamageSource::with_entities(holder, Some(tnt), Some(attacker)),
        &level,
    );
    let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("damage-weapon\t"))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 6);
}

#[test]
fn required_enchantment_armor_query_never_supplies_a_warn_fraction() {
    let mut level = crate::entity::tests::level(false);
    let victim = player(&mut level);
    let attacker = player(&mut level);
    let mut weapon = ItemStack::new(Item::DIAMOND_SWORD, 1, Default::default());
    weapon
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::Enchantments(ItemEnchantments(vec![(
            Enchantment("minecraft:breach".parse().unwrap()),
            VarInt(1),
        )])));
    level
        .entity_mut::<ServerPlayer>(attacker)
        .unwrap()
        .inventory
        .set_selected_item(weapon);
    let source = level.damage_sources().player_attack(attacker);
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert_eq!(
            get_damage_after_absorb(victim, 4.0, &source, 20.0, 8.0, &mut level),
            Err(LevelError::UnsupportedEntityMethod(
                EntityType::PLAYER,
                EntityMethod::ModifyArmorEffectiveness
            )),
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
