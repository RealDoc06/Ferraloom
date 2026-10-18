use serde_json::Value;
use std::fmt::Write;
pub fn generate(out: &mut String, report: &Value, count: usize) {
    let values = report.as_array().expect("block loot table array");
    assert_eq!(values.len(), count);
    writeln!(out, "static BLOCK_LOOT_TABLES: [Option<&str>; {count}] = [").unwrap();
    for value in values {
        match value.as_str() {
            Some(name) => writeln!(out, "Some({name:?}),").unwrap(),
            None => out.push_str("None,\n"),
        }
    }
    out.push_str("];\nimpl Block { /// Port of BlockBehaviour#getLootTable, including shared and absent keys.\npub const fn loot_table_name(self) -> Option<&'static str> { BLOCK_LOOT_TABLES[self.protocol_id() as usize] } }\n");
}
