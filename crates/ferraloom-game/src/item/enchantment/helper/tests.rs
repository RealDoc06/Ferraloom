use super::*;
use crate::item::stack::tests::level;
use ferraloom_data::Item;
use ferraloom_protocol::{
    codec::VarInt,
    component::{DataComponentPatch, Enchantment, ItemEnchantments, ItemStack},
};
use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
use std::io::Read;

thread_local! { static SLOT_SCANS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) }; }
struct SlotProbe;
impl crate::entity::Entity for SlotProbe {
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        refs: &mut Vec<crate::entity::EntityRef>,
    ) {
        crate::entity::player::PlayerBehavior.trace_entity_refs(state, refs);
    }
    fn item_by_slot<'a>(
        &self,
        entity: crate::entity::EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &'a crate::level::Level,
    ) -> Result<Option<&'a ItemStack>, LevelError> {
        SLOT_SCANS
            .with_borrow_mut(|calls| calls.push(crate::entity::player::equipment::slot_name(slot)));
        crate::entity::player::PlayerBehavior.item_by_slot(entity, slot, level)
    }
}

#[test]
fn native_random_item_with_scans_values_order_and_draws_only_for_real_candidates() {
    use crate::entity::{
        EntityState,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_util::{Uuid, Vec3};
    static BEHAVIORS: crate::Behaviors =
        crate::Behaviors::EMPTY.with_entity(ferraloom_data::EntityType::PLAYER, &SlotProbe);
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("orb-repair-scan\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
        level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
        let player = level.next_entity_ref();
        let mut state = ServerPlayer::new(
            Uuid::new(0, 1),
            "selection".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        crate::entity::tests::orb_equipment(&mut state, fields[1].parse().unwrap());
        state.random.set_seed(fields[2].parse().unwrap());
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        SLOT_SCANS.with_borrow_mut(Vec::clear);
        let mut operation = StackOperation::new(&mut level);
        let selected = get_random_item_with(
            EffectComponent::REPAIR_WITH_XP,
            player,
            |stack| {
                crate::item::durability::is_damageable_item(stack)
                    && crate::item::durability::damage_value(stack) > 0
            },
            &mut operation,
        )
        .unwrap();
        assert_eq!(
            selected.map_or("-", |selected| crate::entity::player::equipment::slot_name(
                selected.slot
            )),
            fields[3],
            "{row}"
        );
        if let Some(selected) = selected {
            let expected = if selected.slot == EquipmentSlot::Mainhand {
                ContainerHandle::PlayerInventory(player)
            } else {
                ContainerHandle::LivingEquipment(player)
            };
            assert_eq!(selected.item.container, expected);
            assert_eq!(selected.owner, player);
        }
        assert_eq!(
            SLOT_SCANS.with_borrow_mut(std::mem::take).join(","),
            fields[4],
            "{row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                operation
                    .level
                    .entity_mut::<ServerPlayer>(player)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            fields[5],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 49);
}

#[test]
fn native_conditional_effect_lists_share_the_live_stream_and_keep_list_order() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/items/enchantments/effect-lists.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    crate::level::drop_fixtures::install(&mut level);
    let enchantment = "fixture:effect_list".parse().unwrap();
    let mut cases = 0;
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let values: Vec<serde_json::Value> = serde_json::from_str(fields[1]).unwrap();
        let effects: Vec<_> = values
            .iter()
            .map(|value| ConditionalEffect::decode(value, EnchantmentValueEffect::decode).unwrap())
            .collect();
        let item = Item::from_protocol_id(fields[2].parse().unwrap()).unwrap();
        let count = fields[3].parse().unwrap();
        let enchantment_level = fields[4].parse().unwrap();
        let input = f32::from_bits(fields[5].parse::<i32>().unwrap() as u32);
        let seed = fields[6].parse().unwrap();
        let expected = fields[7].parse::<i32>().unwrap() as u32;
        let next = fields[8].parse::<i64>().unwrap();
        let stack = ItemStack::new(item, count, Default::default());
        *level.random() = Random::Legacy(LegacyRandomSource::new(seed));
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation.own(player, stack).unwrap();
        let result = modify_item_filtered_count(
            &enchantment,
            &effects,
            enchantment_level,
            receiver,
            input,
            &mut operation,
        )
        .unwrap();
        assert_eq!(result.to_bits(), expected, "{line}");
        assert_eq!(operation.level.random().next_long(), next, "{line}");
        assert_eq!(
            inventory::count(operation.level.stack(receiver).unwrap()),
            count,
            "read-only TOOL conditions: {line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 896);
}

#[test]
fn iteration_captures_the_enchantment_component_and_retains_stored_entry_order() {
    let (mut level, player) = level();
    let expected = [("minecraft:unbreaking", 2), ("minecraft:mending", 1)];
    let enchantments = ItemEnchantments(
        expected
            .iter()
            .map(|(id, level)| (Enchantment(id.parse().unwrap()), VarInt(*level)))
            .collect(),
    );
    let mut patch = DataComponentPatch::default();
    patch.set(DataComponent::Enchantments(enchantments));
    let mut operation = StackOperation::new(&mut level);
    let receiver = operation
        .own(player, ItemStack::new(Item::DIAMOND_PICKAXE, 1, patch))
        .unwrap();
    let mut observed = Vec::new();
    run_iteration_on_item(receiver, &mut operation, |id, level, operation| {
        observed.push((id.to_string(), level));
        operation
            .level
            .stack_mut(receiver)
            .unwrap()
            .0
            .as_mut()
            .unwrap()
            .components
            .set(DataComponent::Enchantments(ItemEnchantments(Vec::new())));
        Ok(())
    })
    .unwrap();
    assert_eq!(observed, expected.map(|(id, level)| (id.to_owned(), level)));
    let mut calls = 0;
    run_iteration_on_item(receiver, &mut operation, |_, _, _| {
        calls += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(calls, 0);
}
