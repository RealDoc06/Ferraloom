//! Pure FeatureElement requirements for the dedicated server's VANILLA_SET.
use serde_json::Value;
use std::fmt::Write;
pub fn generate(out: &mut String, report: &Value, blocks: usize, items: usize) {
    for (kind, count) in [("Block", blocks), ("Item", items)] {
        let key = format!("disabled_{}s", kind.to_lowercase());
        let mut enabled = vec![true; count];
        for id in report[&key].as_array().expect("disabled feature ids") {
            enabled[id.as_u64().expect("registry id") as usize] = false;
        }
        let table = format!("{}_ENABLED_IN_VANILLA", kind.to_uppercase());
        writeln!(out, "static {table}: [bool; {count}] = {enabled:?};").unwrap();
        writeln!(out, "impl {kind} {{ /// Port of FeatureElement#isEnabled(FeatureFlags.VANILLA_SET).\npub const fn is_enabled_in_vanilla(self) -> bool {{ {table}[self.protocol_id() as usize] }} }}").unwrap();
    }
    let mut placements = vec![false; items];
    for id in report["block_placement_attempt_items"]
        .as_array()
        .expect("placement item ids")
    {
        placements[id.as_u64().expect("registry id") as usize] = true;
    }
    writeln!(
        out,
        "static BLOCK_PLACEMENT_ATTEMPT_ITEMS: [bool; {items}] = {placements:?};"
    )
    .unwrap();
    writeln!(out, "impl Item {{ /// Item-class predicate from ServerGamePacketListenerImpl#wasBlockPlacementAttempt; the caller checks empty stacks and cooldowns.\npub const fn is_block_placement_attempt(self) -> bool {{ BLOCK_PLACEMENT_ATTEMPT_ITEMS[self.protocol_id() as usize] }} }}").unwrap();
    for (key, method) in [
        ("sign_applicator_items", "is_sign_applicator"),
        ("hanging_sign_items", "is_hanging_sign_item"),
    ] {
        let mut values = vec![false; items];
        for id in report[key].as_array().expect("item-class registry ids") {
            values[id.as_u64().expect("registry id") as usize] = true;
        }
        let table = key.to_uppercase();
        writeln!(out, "static {table}: [bool; {items}] = {values:?};").unwrap();
        writeln!(out, "impl Item {{ /// Native item-class predicate used by SignBlock#useItemOn.\npub const fn {method}(self) -> bool {{ {table}[self.protocol_id() as usize] }} }}").unwrap();
    }
    for (key, method) in [
        (
            "stacked_on_other_override_items",
            "overrides_stacked_on_other",
        ),
        (
            "other_stacked_on_me_override_items",
            "overrides_other_stacked_on_me",
        ),
    ] {
        let mut values = vec![false; items];
        for id in report[key].as_array().expect("inventory click method ids") {
            values[id.as_u64().expect("registry id") as usize] = true;
        }
        let table = key.to_uppercase();
        writeln!(out, "static {table}: [bool; {items}] = {values:?};").unwrap();
        writeln!(out, "impl Item {{ /// Native virtual ownership used by AbstractContainerMenu#tryItemClickBehaviourOverride.\npub const fn {method}(self) -> bool {{ {table}[self.protocol_id() as usize] }} }}").unwrap();
    }
}
