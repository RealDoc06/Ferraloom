use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Only values deliberately recorded by the scenario are compared. Labels must be unique.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ObservationLog(pub BTreeMap<String, Value>);
impl ObservationLog {
    pub fn record(&mut self, label: impl Into<String>, value: impl Serialize) -> Result<()> {
        let label = label.into();
        ensure!(
            !self.0.contains_key(&label),
            "duplicate observation label {label}"
        );
        self.0.insert(label, serde_json::to_value(value)?);
        Ok(())
    }
    pub fn compare(&self, actual: &Self) -> Result<()> {
        let expected = serde_json::to_value(self)?;
        let actual = serde_json::to_value(actual)?;
        let mut differences = Vec::new();
        diff("", &expected, &actual, &mut differences);
        if differences.is_empty() {
            Ok(())
        } else {
            bail!(
                "parity observations differ (vanilla -> Ferraloom):\n{}",
                differences.join("\n")
            )
        }
    }
    pub fn normalized(&self, rules: &[Normalization]) -> Result<Self> {
        let mut value = serde_json::to_value(self)?;
        let mut entities = BTreeMap::new();
        let mut uuids = BTreeMap::new();
        let mut selected = std::collections::BTreeSet::new();
        for rule in rules {
            let pointer = match rule {
                Normalization::EntityId(p)
                | Normalization::RandomUuid(p)
                | Normalization::Timestamp(p)
                | Normalization::VaryingSeed(p) => p,
            };
            ensure!(
                selected.insert(pointer),
                "duplicate normalization path {pointer}"
            );
            let entry = value.pointer_mut(pointer).with_context(|| {
                format!("normalization points to missing observation: {pointer}")
            })?;
            match rule {
                Normalization::EntityId(_) => {
                    let raw = entry.as_i64().context("entity id is not an integer")?;
                    let next = entities.len() as i64;
                    *entry = Value::from(*entities.entry(raw).or_insert(next));
                }
                Normalization::RandomUuid(_) => {
                    let raw = match entry {
                        Value::String(raw) => {
                            // Preserve the existing string validation and identity keys.
                            ensure!(
                                raw.len() == 36
                                    && raw.bytes().enumerate().all(|(i, b)| {
                                        if [8, 13, 18, 23].contains(&i) {
                                            b == b'-'
                                        } else {
                                            b.is_ascii_hexdigit()
                                        }
                                    }),
                                "invalid UUID at {pointer}"
                            );
                            raw.clone()
                        }
                        Value::Array(words) => {
                            ensure!(
                                words.len() == 4,
                                "UUID must have four integers at {pointer}"
                            );
                            let mut decoded = [0; 4];
                            for (target, word) in decoded.iter_mut().zip(words) {
                                *target = word
                                    .as_i64()
                                    .and_then(|word| i32::try_from(word).ok())
                                    .with_context(|| {
                                    format!("UUID word is not a signed integer at {pointer}")
                                })?;
                            }
                            ferraloom_util::Uuid::from_int_array(decoded).to_string()
                        }
                        _ => bail!("UUID is not a string or four integers at {pointer}"),
                    };
                    let next = uuids.len();
                    *entry = Value::from(format!("uuid-{}", uuids.entry(raw).or_insert(next)));
                }
                Normalization::Timestamp(_) | Normalization::VaryingSeed(_) => {
                    ensure!(
                        entry.is_number(),
                        "timestamp/seed must be numeric at {pointer}"
                    );
                    *entry = Value::from(0);
                }
            }
        }
        Ok(serde_json::from_value(value)?)
    }
}
/// Exact JSON pointers into the observation log. Missing fields and wrong types are errors.
/// Repeated raw identities map to the same identity across all selected fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Normalization {
    EntityId(String),
    RandomUuid(String),
    Timestamp(String),
    VaryingSeed(String),
}
fn diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                let path = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => diff(&path, a, b, out),
                    (a, b) => out.push(format!("{path}: {a:?} -> {b:?}")),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}/{i}"), a, b, out)
            }
        }
        _ if a != b => out.push(format!("{path}: {a} -> {b}")),
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalization_is_explicit_and_preserves_identity_relationships() -> Result<()> {
        let mut a = ObservationLog::default();
        let mut b = ObservationLog::default();
        a.record("ids", [7, 7, 9])?;
        b.record("ids", [42, 42, 81])?;
        assert!(a.compare(&b).is_err());
        let rules = (0..3)
            .map(|i| Normalization::EntityId(format!("/ids/{i}")))
            .collect::<Vec<_>>();
        a.normalized(&rules)?.compare(&b.normalized(&rules)?)?;
        b.0.insert("ids".into(), serde_json::json!([42, 81, 81]));
        assert!(
            a.normalized(&rules)?
                .compare(&b.normalized(&rules)?)
                .is_err()
        );
        assert!(
            a.normalized(&[Normalization::Timestamp("/missing".into())])
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn uuid_normalization_shares_string_and_native_int_array_identity_only_at_selected_fields()
    -> Result<()> {
        let words = [i32::MIN, 0, 0, 1];
        let uuid = ferraloom_util::Uuid::from_int_array(words).to_string();
        let mut log = ObservationLog::default();
        log.record(
            "entity",
            serde_json::json!({
                "packet_uuid":uuid, "hover_uuid":words, "insertion":uuid,
                "unrelated_array":words, "other_uuid":"ffffffff-ffff-ffff-ffff-ffffffffffff",
            }),
        )?;
        let rules = ["packet_uuid", "hover_uuid", "insertion", "other_uuid"]
            .map(|field| Normalization::RandomUuid(format!("/entity/{field}")));
        let normalized = log.normalized(&rules)?;
        ensure!(
            normalized.0["entity"]
                == serde_json::json!({
                    "packet_uuid":"uuid-0", "hover_uuid":"uuid-0", "insertion":"uuid-0",
                    "unrelated_array":words, "other_uuid":"uuid-1",
                })
        );
        for invalid in [
            serde_json::json!([1, 2, 3]),
            serde_json::json!([0, 0, 0, 2147483648_i64]),
            serde_json::json!([0, 0, 0, 0.5]),
        ] {
            log.record("invalid", invalid)?;
            ensure!(
                log.normalized(&[Normalization::RandomUuid("/invalid".into())])
                    .is_err()
            );
            log.0.remove("invalid");
        }
        Ok(())
    }

    #[test]
    fn diff_names_the_changed_field() -> Result<()> {
        let mut a = ObservationLog::default();
        let mut b = ObservationLog::default();
        a.record("player", serde_json::json!({"position":[1,2,3]}))?;
        b.record("player", serde_json::json!({"position":[1,4,3]}))?;
        assert!(
            a.compare(&b)
                .unwrap_err()
                .to_string()
                .contains("/player/position/1: 2 -> 4")
        );
        Ok(())
    }
}
