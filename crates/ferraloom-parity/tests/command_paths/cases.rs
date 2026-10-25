use super::harness::{Case, Check, Step};
use ferraloom_parity::Result;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Rule {
    name: String,
    default: Value,
    enabled: bool,
    min: Option<i32>,
    max: Option<i32>,
}
fn argument(mut step: Step, key: &str, index: usize, value: Value) -> Step {
    step.checks.push(Check::Argument {
        key: key.into(),
        index,
        value,
    });
    step
}
fn gamerules() -> Result<Vec<Case>> {
    let rules: Vec<Rule> = serde_json::from_str(include_str!(
        "../../../ferraloom-game/src/commands/vanilla/rules.json"
    ))?;
    let mut cases = Vec::new();
    for rule in rules.into_iter().filter(|rule| rule.enabled) {
        let default = rule.default.to_string();
        let alternate = match &rule.default {
            Value::Bool(value) => (!value).to_string(),
            Value::Number(value) => {
                let value = value.as_i64().unwrap() as i32;
                if value > rule.min.unwrap_or(i32::MIN) {
                    value - 1
                } else {
                    value + 1
                }
                .to_string()
            }
            _ => unreachable!("native game rule value type"),
        };
        for name in [
            rule.name.strip_prefix("minecraft:").unwrap(),
            rule.name.as_str(),
        ] {
            let path = format!("commands.path.gamerule.{}", name.replace(['_', ':'], "-"));
            cases.push(Case {
                name: format!("{path}.query"),
                path: Some(path.clone()),
                pending: None,
                steps: vec![argument(
                    Step::feedback(format!("gamerule {name}"), "commands.gamerule.query"),
                    "commands.gamerule.query",
                    1,
                    Value::String(default.clone()),
                )],
            });
            let setter = if name.ends_with("send_command_feedback") && alternate == "false" {
                let mut step = Step::feedback(format!("gamerule {name} false"), "");
                step.keys.clear();
                step.silent = true;
                step
            } else {
                argument(
                    Step::feedback(
                        format!("gamerule {name} {alternate}"),
                        "commands.gamerule.set",
                    ),
                    "commands.gamerule.set",
                    1,
                    Value::String(alternate.clone()),
                )
            };
            let mut steps = vec![setter];
            if !(name.ends_with("send_command_feedback") && alternate == "false") {
                steps.push(argument(
                    Step::feedback(format!("gamerule {name}"), "commands.gamerule.query"),
                    "commands.gamerule.query",
                    1,
                    Value::String(alternate.clone()),
                ));
                steps.push(Step::feedback(
                    format!("gamerule {name} {alternate}"),
                    "commands.gamerule.not_set",
                ));
            }
            // A successful true restoration also proves the silent false setter
            // changed state: restoring an unchanged rule is a native failure.
            steps.push(argument(
                Step::feedback(
                    format!("gamerule {name} {default}"),
                    "commands.gamerule.set",
                ),
                "commands.gamerule.set",
                1,
                Value::String(default.clone()),
            ));
            if let Some(min) = rule.min
                && min > i32::MIN
            {
                steps.push(Step::feedback(
                    format!("gamerule {name} {}", min - 1),
                    "argument.integer.low",
                ));
            }
            if let Some(max) = rule.max
                && max < i32::MAX
            {
                steps.push(Step::feedback(
                    format!("gamerule {name} {}", max + 1),
                    "argument.integer.big",
                ));
            }
            cases.push(Case {
                name: format!("{path}.set"),
                path: Some(format!("{path}.arg-value")),
                pending: name
                    .ends_with("locator_bar")
                    .then(|| "waypoint service".into()),
                steps,
            });
        }
    }
    Ok(cases)
}

#[derive(Deserialize)]
struct DataVector {
    command: String,
    path: String,
    initial: String,
    source: String,
    value: Option<i32>,
    error: Option<Value>,
    feedback: Vec<Value>,
}
fn data_storage() -> Result<Vec<Case>> {
    let vectors: Vec<DataVector> = serde_json::from_str(include_str!(
        "../../../ferraloom-game/src/commands/vanilla/data-command-vectors.json"
    ))?;
    let mut cases = Vec::new();
    for (index, vector) in vectors.into_iter().enumerate() {
        // Unique keys let every case begin with the native fixture's complete
        // state in one shared boot, without relying on a non-native reset API.
        let root = format!("paths:case/{index}/root");
        let source = format!("paths:case/{index}/source");
        let absent = format!("paths:case/{index}/absent");
        let command = vector
            .command
            .replace("test:root", &root)
            .replace("test:source", &source)
            .replace("test:absent", &absent);
        let key = vector
            .error
            .as_ref()
            .and_then(|error| error.get("translate"))
            .or_else(|| vector.feedback.first()?.get("message")?.get("translate"))
            .and_then(Value::as_str)
            .expect("native data feedback key");
        cases.push(Case {
            name: format!("data.storage.{index}"),
            // Errors remain checked, but only successful native executions can
            // grant path coverage. Readbacks observe actual post-command NBT.
            path: vector.value.map(|_| vector.path),
            pending: None,
            steps: vec![
                Step::feedback(
                    format!("data merge storage {root} {}", vector.initial),
                    "commands.data.storage.modified",
                ),
                Step::feedback(
                    format!("data merge storage {source} {}", vector.source),
                    "commands.data.storage.modified",
                ),
                Step::feedback(command, key),
                Step::feedback(
                    format!("data get storage {root}"),
                    "commands.data.storage.query",
                ),
                Step::feedback(
                    format!("data get storage {absent}"),
                    "commands.data.storage.query",
                ),
            ],
        });
    }
    Ok(cases)
}
pub fn family(name: &str) -> Result<Vec<Case>> {
    if name == "data_storage" {
        return data_storage();
    }
    if name == "gamerule" {
        return gamerules();
    }
    let all: std::collections::BTreeMap<String, Vec<Case>> =
        serde_json::from_str(include_str!("cases.json"))?;
    all.get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("unknown command family {name}"))
}

#[test]
fn every_sweep_path_has_an_inventory_row_and_an_observable_command() -> Result<()> {
    let inventory = include_str!("../../../../features/commands.md");
    let ids = inventory
        .lines()
        .filter_map(|line| line.strip_prefix("| ")?.split_once(" | ").map(|(id, _)| id))
        .collect::<std::collections::BTreeSet<_>>();
    for name in [
        "gamerule",
        "give",
        "clocks",
        "chat",
        "players",
        "admin",
        "placement",
        "data_storage",
    ] {
        let mut labels = std::collections::BTreeSet::new();
        for case in family(name)? {
            assert!(
                labels.insert(case.name.clone()),
                "duplicate case {}",
                case.name
            );
            assert!(!case.steps.is_empty());
            if let Some(path) = &case.path {
                assert!(ids.contains(path.as_str()), "unknown feature path {path}");
                assert!(
                    !["scoreboard", "team", "bossbar", "trigger"]
                        .contains(&path.split('.').nth(2).unwrap()),
                    "other owner's row {path}"
                );
            }
            for path in case.paths() {
                assert!(ids.contains(path.as_str()), "missing prefix {path}");
            }
            assert!(
                case.steps.iter().all(|step| !step.keys.is_empty()
                    || step.silent
                    || step.chat
                    || step.literal_prefix.is_some()),
                "unobserved command in {}",
                case.name
            );
        }
    }
    Ok(())
}

#[test]
fn native_successful_substrings_do_not_cover_the_always_failing_string_root() -> Result<()> {
    let covered = data_storage()?
        .iter()
        .flat_map(Case::paths)
        .collect::<std::collections::BTreeSet<_>>();
    let root = "commands.path.data.modify.storage.arg-target.arg-target-path.set.string.storage.arg-source";
    assert!(!covered.contains(root));
    assert!(covered.contains(&format!("{root}.arg-source-path")));
    Ok(())
}
