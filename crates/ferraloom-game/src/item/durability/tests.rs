use super::*;
use crate::item::stack::tests::level;
use ferraloom_data::Item;
use ferraloom_protocol::component::{DataComponentPatch, Unit};
use std::io::Read;

fn fixture(kind: usize, count: i32) -> ItemStack {
    let mut patch = DataComponentPatch::default();
    if kind != 0 {
        patch.set(DataComponent::MaxDamage(VarInt(7)));
        patch.set(DataComponent::Damage(VarInt(2)));
    }
    match kind {
        2 => patch.set(DataComponent::Unbreakable(Unit)),
        3 => patch.remove(ComponentTypeId::DAMAGE),
        4 => patch.remove(ComponentTypeId::MAX_DAMAGE),
        5 => patch.set(DataComponent::MaxDamage(VarInt(-3))),
        6 => patch.set(DataComponent::MaxDamage(VarInt(0))),
        _ => {}
    }
    let mut stack = ItemStack::new(
        if kind == 7 {
            Item::STONE
        } else {
            Item::DIAMOND_PICKAXE
        },
        count.max(1),
        patch,
    );
    inventory::set_count(&mut stack, count);
    stack
}
fn snapshot(stack: &ItemStack) -> String {
    format!(
        "{},{},{},{},{},{},{}",
        stack.item().protocol_id(),
        inventory::count(stack),
        damage_value(stack),
        max_damage(stack),
        is_damageable_item(stack),
        is_broken(stack),
        next_damage_will_break(stack)
    )
}

#[test]
fn native_apply_damage_clamps_copies_before_shrink_and_runs_break_callbacks_synchronously() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/enchantments/durability-mutation.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    let mut cases = 0;
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let stack = fixture(fields[1].parse().unwrap(), fields[2].parse().unwrap());
        assert_eq!(snapshot(&stack), fields[5], "{line}");
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation.own(player, stack).unwrap();
        let reenter = fields[4].parse::<bool>().unwrap();
        let mut callbacks = Vec::new();
        apply_damage(
            receiver,
            fields[3].parse().unwrap(),
            None,
            &mut operation,
            |broken, operation| {
                let copy_before = snapshot(&broken);
                let live_before = snapshot(operation.level.stack(receiver).unwrap());
                if reenter {
                    let live = operation.level.stack_mut(receiver).unwrap();
                    inventory::set_count(live, 2);
                    set_damage_value(live, 1);
                }
                callbacks.push(format!(
                    "{}#{}#{}#{}#false",
                    copy_before,
                    live_before,
                    snapshot(&broken),
                    snapshot(operation.level.stack(receiver).unwrap())
                ));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            snapshot(operation.level.stack(receiver).unwrap()),
            fields[6],
            "{line}"
        );
        assert_eq!(callbacks.join(";"), fields[7], "{line}");
        cases += 1;
    }
    assert_eq!(cases, 432);
}
