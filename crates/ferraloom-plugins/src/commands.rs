//! Plugins' commands in the command tree every player receives.

use serde_json::{Value, json};

/// The commands report's form (`horizon-client/src/lan/commands.rs` reads
/// it) of plugin command labels: each a literal that runs alone or with a
/// greedy string argument `args`, as CraftBukkit's `BukkitCommandWrapper`
/// sends them, so a client's chat box accepts any arguments after it.
/// `Value::Null` when there are none.
pub fn command_tree<S: AsRef<str>>(labels: &[S]) -> Value {
    let mut children = serde_json::Map::new();
    for label in labels {
        let label = label.as_ref();
        if label.is_empty() || label.contains(' ') {
            continue;
        }
        children.insert(
            label.to_owned(),
            json!({
                "type": "literal",
                "executable": true,
                "children": {
                    "args": {
                        "type": "argument",
                        "parser": "brigadier:string",
                        "properties": {"type": "greedy"},
                        "executable": true,
                    }
                }
            }),
        );
    }
    if children.is_empty() {
        return Value::Null;
    }
    json!({"type": "root", "children": children})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_label_takes_any_arguments() {
        let tree = command_tree(&["ftest", "ferraloomtest:ftest", "bad label"]);
        let children = tree["children"].as_object().unwrap();
        assert_eq!(children.len(), 2);
        let ftest = &children["ftest"];
        assert_eq!(ftest["executable"], true);
        assert_eq!(ftest["children"]["args"]["parser"], "brigadier:string");
        assert_eq!(ftest["children"]["args"]["properties"]["type"], "greedy");
        assert_eq!(command_tree::<&str>(&[]), Value::Null);
    }
}
