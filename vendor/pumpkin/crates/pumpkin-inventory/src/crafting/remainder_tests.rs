use super::reconcile_remainder;
use pumpkin_data::{data_component_impl::CustomDataImpl, item::Item, item_stack::ItemStack};
use pumpkin_nbt::compound::NbtCompound;

// The fixed builtin mapping never returns the ingredient item itself. Exercise this otherwise
// unreachable generic Java merge rule directly, without inventing a builtin recipe.
#[test]
fn same_item_remainder_merges_survivor_above_stack_limit() {
    let mut survivor = ItemStack::new(64, &Item::GLASS_BOTTLE);
    let spill = reconcile_remainder(&mut survivor, ItemStack::new(1, &Item::GLASS_BOTTLE));
    assert!(spill.is_empty());
    assert_eq!(survivor.item_count, 65);
    assert!(survivor.are_items_and_components_equal(&ItemStack::new(1, &Item::GLASS_BOTTLE)));
}

#[test]
fn same_item_remainder_with_different_components_spills_unchanged() {
    let mut data = NbtCompound::new();
    data.put_int("survivor", 1);
    let mut survivor = ItemStack::new(12, &Item::GLASS_BOTTLE);
    survivor.set_data_component(CustomDataImpl::new(data));
    let original = survivor.clone();
    let remainder = ItemStack::new(1, &Item::GLASS_BOTTLE);
    let spill = reconcile_remainder(&mut survivor, remainder.clone());
    assert_eq!(survivor.item_count, original.item_count);
    assert!(survivor.are_items_and_components_equal(&original));
    assert_eq!(spill.item_count, 1);
    assert!(spill.are_items_and_components_equal(&remainder));
}
