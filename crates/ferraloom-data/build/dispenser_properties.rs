//! Fixed registry membership, distinct from runtime per-stack behavior selection.
use std::{collections::BTreeSet, fmt::Write, path::Path};
fn variant(class: &str) -> String {
    class.rsplit('.').next().unwrap().replace('$', "")
}
pub fn generate(root: &Path) {
    let data = super::load(root, "dispenser_properties");
    let entries = data.as_array().unwrap();
    let mut kinds: BTreeSet<_> = entries
        .iter()
        .filter_map(|entry| entry["behavior"].as_str())
        .collect();
    kinds.insert("Noop");
    let mut out = String::from(
        "/// Concrete classes in vanilla's bootstrapped DISPENSER_REGISTRY.\n#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\npub enum DispenseBehaviorKind {\n",
    );
    let mut variants = BTreeSet::new();
    for &kind in &kinds {
        let name = variant(kind);
        assert!(variants.insert(name.clone()), "dispenser class collision");
        writeln!(out, "{name},").unwrap();
    }
    out.push_str("}\nimpl DispenseBehaviorKind { pub const fn java_class(self) -> &'static str { match self {\n");
    for &kind in &kinds {
        writeln!(out, "Self::{} => {kind:?},", variant(kind)).unwrap();
    }
    out.push_str("} } }\nconst DISPENSER_PROPERTIES: &[DispenserProperties] = &[\n");
    for (id, entry) in entries.iter().enumerate() {
        assert_eq!(entry["id"].as_u64().unwrap(), id as u64);
        write!(
            out,
            "DispenserProperties {{ spawn_egg: {}, registered_behavior: ",
            entry["spawn_egg"]
        )
        .unwrap();
        if let Some(kind) = entry["behavior"].as_str() {
            write!(out, "Some(DispenseBehaviorKind::{}),", variant(kind)).unwrap();
        } else {
            out.push_str("None,");
        }
        let config = &entry["projectile"];
        if config.is_null() {
            out.push_str("projectile: None,");
        } else {
            write!(out,"projectile: Some(ProjectileDispenseConfig {{ power: {:?}_f32, uncertainty: {:?}_f32, position: ProjectileDispensePosition::{}, override_dispense_event: ", config["power"].as_f64().unwrap(),config["uncertainty"].as_f64().unwrap(),config["position"].as_str().unwrap()).unwrap();
            if let Some(event) = config["event"].as_i64() {
                write!(out, "Some({event})").unwrap();
            } else {
                out.push_str("None");
            }
            out.push_str(" }),");
        }
        out.push_str("},\n");
    }
    out.push_str("];\n");
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("dispenser_properties.rs"),
        out,
    )
    .unwrap();
}
