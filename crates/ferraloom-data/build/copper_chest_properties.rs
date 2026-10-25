use serde_json::Value;
use std::fmt::Write;

pub fn generate(out: &mut String, report: &Value, block_count: usize) {
    let mut rows = vec![None; block_count];
    for row in report.as_array().expect("copper chest properties") {
        let id = row["block"].as_u64().unwrap() as usize;
        assert!(rows[id].replace(row).is_none(), "duplicate copper chest");
    }
    writeln!(
        out,
        "static COPPER_CHEST_PROPERTIES: [Option<CopperChestProperties>; {block_count}] = ["
    )
    .unwrap();
    for row in rows {
        let Some(row) = row else {
            out.push_str("None,\n");
            continue;
        };
        let stage = row["weathering_stage"].as_u64().unwrap();
        assert!(stage < 4);
        let waxed = row["is_waxed"].as_bool().unwrap();
        let unwaxed = row["unwaxed"]
            .as_u64()
            .map_or_else(|| "None".to_owned(), |id| format!("Some(Block({id}))"));
        writeln!(out, "Some(CopperChestProperties {{ weathering_stage: {stage}, is_waxed: {waxed}, unwaxed: {unwaxed} }}),").unwrap();
    }
    out.push_str("];\n");
}
