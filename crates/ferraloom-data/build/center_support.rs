//! Cached CENTER support is initialized once by BlockStateBase.Cache.
use serde_json::Value;
use std::fmt::Write;

pub fn generate(out: &mut String, values: &Value, state_count: usize) {
    let values = values.as_array().expect("center support cache array");
    assert_eq!(values.len(), state_count);
    write!(out, "static CENTER_SUPPORT: [u8; {state_count}] = [").unwrap();
    for value in values {
        let bits = value.as_u64().expect("center support bits");
        assert!(bits == 0 || (64..128).contains(&bits));
        write!(out, "{bits},").unwrap();
    }
    out.push_str("];\n");
}
