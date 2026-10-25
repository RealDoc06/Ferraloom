//! Immutable Java item class hierarchy and exact virtual-method ownership.
use serde_json::Value;
use std::{collections::BTreeSet, fmt::Write, path::Path};

pub fn generate(root: &Path, item_report: &Value) {
    let data = super::load(root, "item_methods");
    let classes = data["classes"].as_array().unwrap();
    let methods = data["methods"].as_array().unwrap();
    let items = data["items"].as_array().unwrap();
    assert_eq!(items.len(), item_report.as_array().unwrap().len());
    let names = classes
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    let variants = names
        .iter()
        .map(|name| super::pascal(name.rsplit('.').next().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        variants.iter().collect::<BTreeSet<_>>().len(),
        variants.len(),
        "item class name collision"
    );
    let mut out = String::from(
        "#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\n#[repr(u16)]\npub enum ItemClass {\n",
    );
    for (variant, name) in variants.iter().zip(&names) {
        writeln!(out, "/// `{name}`.\n{variant},").unwrap();
    }
    out.push_str("}\nimpl ItemClass { pub const ALL: &[Self] = &[");
    for variant in &variants {
        write!(out, "Self::{variant},").unwrap();
    }
    out.push_str(
        "]; pub const fn java_name(self) -> &'static str { ITEM_CLASS_NAMES[self as usize] } }\n",
    );
    writeln!(
        out,
        "const ITEM_CLASS_NAMES: [&str; {}] = {names:?};",
        names.len()
    )
    .unwrap();
    out.push_str("#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\n#[repr(u8)]\npub enum ItemMethod {\n");
    let method_variants = methods
        .iter()
        .map(|m| {
            super::pascal(
                m["rust_name"]
                    .as_str()
                    .unwrap_or_else(|| m["name"].as_str().unwrap()),
            )
        })
        .collect::<Vec<_>>();
    assert!(methods.len() <= u8::MAX as usize);
    assert_eq!(
        method_variants.iter().collect::<BTreeSet<_>>().len(),
        method_variants.len()
    );
    for (method, variant) in methods.iter().zip(&method_variants) {
        let parameters = method["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect::<Vec<_>>()
            .join(", ");
        let owner = names[method["base"].as_u64().unwrap() as usize];
        writeln!(
            out,
            "/// `{owner}#{}({parameters})`.\n{variant},",
            method["name"].as_str().unwrap()
        )
        .unwrap();
    }
    out.push_str("}\nimpl ItemMethod { pub const ALL: &[Self] = &[");
    for variant in &method_variants {
        write!(out, "Self::{variant},").unwrap();
    }
    out.push_str("]; pub const fn base(self) -> ItemClass { ITEM_METHOD_BASES[self as usize] }\n");
    out.push_str("/// None means this signature is not part of the item's hierarchy.\npub const fn owner(self, item: Item) -> Option<ItemClass> { ITEM_METHOD_OWNERS[item.protocol_id() as usize][self as usize] }\n}\n");
    write!(
        out,
        "const ITEM_METHOD_BASES: [ItemClass; {}] = [",
        methods.len()
    )
    .unwrap();
    for method in methods {
        write!(
            out,
            "ItemClass::{},",
            variants[method["base"].as_u64().unwrap() as usize]
        )
        .unwrap();
    }
    out.push_str("];\nconst ITEM_CLASSES: &[ItemClass] = &[\n");
    for (item, report) in items.iter().zip(item_report.as_array().unwrap()) {
        assert_eq!(item["name"], report["name"]);
        writeln!(
            out,
            "ItemClass::{},",
            variants[item["class"].as_u64().unwrap() as usize]
        )
        .unwrap();
    }
    out.push_str("];\n");
    let words = classes.len().div_ceil(64);
    writeln!(
        out,
        "const ITEM_CLASS_INSTANCES: [[u64; {words}]; {}] = [",
        classes.len()
    )
    .unwrap();
    for class in classes {
        let mut bits = vec![0u64; words];
        for parent in class["instances"].as_array().unwrap() {
            let index = parent.as_u64().unwrap() as usize;
            assert!(index < classes.len());
            bits[index / 64] |= 1 << (index % 64);
        }
        writeln!(out, "{bits:?},").unwrap();
    }
    out.push_str("];\n");
    writeln!(
        out,
        "static ITEM_METHOD_OWNERS: [[Option<ItemClass>; {}]; {}] = [",
        methods.len(),
        items.len()
    )
    .unwrap();
    for item in items {
        let owners = item["declaring_classes"].as_array().unwrap();
        assert_eq!(owners.len(), methods.len());
        out.push('[');
        for owner in owners {
            if let Some(index) = owner.as_u64() {
                write!(out, "Some(ItemClass::{}),", variants[index as usize]).unwrap();
            } else {
                out.push_str("None,");
            }
        }
        out.push_str("],\n");
    }
    out.push_str("];\n");
    for (field, ty, conversion) in [
        (
            "wall_block",
            "crate::Block",
            "crate::Block::from_protocol_id",
        ),
        (
            "attachment_direction",
            "ferraloom_util::Direction",
            "direction",
        ),
        (
            "place_sound",
            "crate::registries::sound_event::Id",
            "crate::registries::sound_event::Id::from_protocol_id",
        ),
    ] {
        writeln!(
            out,
            "const ITEM_{}: &[Option<{ty}>] = &[",
            field.to_uppercase()
        )
        .unwrap();
        for item in items {
            if let Some(value) = item[field].as_u64() {
                if conversion == "direction" {
                    assert!(value < 6);
                    writeln!(out, "Some(ferraloom_util::Direction::VALUES[{value}]),").unwrap();
                } else {
                    writeln!(out, "Some({conversion}({value}).unwrap()),").unwrap();
                }
            } else {
                out.push_str("None,\n");
            }
        }
        out.push_str("];\n");
    }
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("item_methods.rs"),
        out,
    )
    .unwrap();
}
