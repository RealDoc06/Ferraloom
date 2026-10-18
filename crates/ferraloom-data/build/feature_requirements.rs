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
}
