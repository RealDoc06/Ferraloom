use serde_json::Value;
use std::fmt::Write;

pub fn generate(out: &mut String, report: &Value, block_count: usize) {
    let mut flags = vec![None; block_count];
    for row in report.as_array().expect("command block properties") {
        let id = row["block"].as_u64().unwrap() as usize;
        assert!(
            flags[id]
                .replace(row["automatic"].as_bool().unwrap())
                .is_none()
        );
    }
    writeln!(
        out,
        "static COMMAND_BLOCK_AUTOMATIC: [Option<bool>; {block_count}] = ["
    )
    .unwrap();
    for flag in flags {
        match flag {
            Some(value) => writeln!(out, "Some({value}),").unwrap(),
            None => out.push_str("None,\n"),
        }
    }
    out.push_str("];\nimpl Block { /// CommandBlock's constructor default, independent of the current BE state.\npub const fn command_block_automatic(self) -> Option<bool> { COMMAND_BLOCK_AUTOMATIC[self.protocol_id() as usize] } }\n");
}
