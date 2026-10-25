//! Immutable notification radii from the native built-in GameEvent registry.
use serde_json::Value;
use std::fmt::Write;

pub fn generate(out: &mut String, facts: &Value, report: &Value) {
    let facts = facts.as_array().expect("game event facts");
    let entries = super::ordered_entries(&report["entries"]);
    assert_eq!(facts.len(), entries.len());
    write!(
        out,
        "static GAME_EVENT_NOTIFICATION_RADII: [i32; {}] = [",
        facts.len()
    )
    .unwrap();
    for (fact, (name, _)) in facts.iter().zip(entries) {
        assert_eq!(fact["name"].as_str(), Some(name));
        let radius = i32::try_from(fact["notification_radius"].as_i64().unwrap()).unwrap();
        write!(out, "{radius},").unwrap();
    }
    out.push_str("];\n");
}
