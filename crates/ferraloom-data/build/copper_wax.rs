use serde_json::Value;
use std::fmt::Write;

/// Native HoneycombItem.WAXABLES is a constructor-only bijection. Generate both
/// lookup directions from that single capture; neither depends on world state.
pub fn generate(out: &mut String, report: &Value, block_count: usize) {
    let mut waxed = vec![None; block_count];
    let mut unwaxed = vec![None; block_count];
    for pair in report.as_array().expect("native copper wax pairs") {
        let from = pair["unwaxed"].as_u64().unwrap() as usize;
        let to = pair["waxed"].as_u64().unwrap() as usize;
        assert!(waxed[from].replace(to).is_none(), "duplicate unwaxed block");
        assert!(unwaxed[to].replace(from).is_none(), "duplicate waxed block");
    }
    for (name, entries) in [("COPPER_WAXED", waxed), ("COPPER_UNWAXED", unwaxed)] {
        writeln!(out, "static {name}: [Option<Block>; {block_count}] = [").unwrap();
        for value in entries {
            match value {
                Some(id) => writeln!(out, "Some(Block({id})),").unwrap(),
                None => out.push_str("None,\n"),
            }
        }
        out.push_str("];\n");
    }
}
