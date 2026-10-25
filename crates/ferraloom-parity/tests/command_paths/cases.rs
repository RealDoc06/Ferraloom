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
                steps,
            });
        }
    }
    Ok(cases)
}
pub fn family(name: &str) -> Result<Vec<Case>> {
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
