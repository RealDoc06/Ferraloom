//! Exact Java implementation ownership for the shared block dispatch table.
use serde_json::Value;
use std::{fmt::Write, path::Path};

pub fn generate(root: &Path, block_properties: &Value) {
    let data = super::load(root, "block_methods");
    let methods = data["methods"].as_array().unwrap();
    let blocks = data["blocks"].as_array().unwrap();
    assert!(methods.len() <= 64);
    assert_eq!(
        blocks.len(),
        block_properties["blocks"].as_array().unwrap().len()
    );
    let mut out = String::from(
        "/// Vanilla virtual method signatures, identified independently of method bodies.\n\
         /// An inherited base method may still delegate to other overridden methods.\n\
         #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]\n\
         #[repr(u8)]\npub enum BlockMethod {\n",
    );
    let variants: Vec<_> = methods
        .iter()
        .map(|method| {
            let name = method["name"].as_str().unwrap();
            format!("{}{}", name[..1].to_ascii_uppercase(), &name[1..])
        })
        .collect();
    for (method, variant) in methods.iter().zip(&variants) {
        let parameters = method["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            out,
            "/// `{}#{}({parameters})`.\n{variant},",
            method["base"].as_str().unwrap(),
            method["name"].as_str().unwrap()
        )
        .unwrap();
    }
    out.push_str("}\nimpl BlockMethod { pub const ALL: &[Self] = &[\n");
    for variant in &variants {
        writeln!(out, "Self::{variant},").unwrap();
    }
    out.push_str("]; }\nconst BLOCK_METHOD_OVERRIDES: &[u64] = &[\n");
    for (block, facts) in blocks
        .iter()
        .zip(block_properties["blocks"].as_array().unwrap())
    {
        assert_eq!(block["name"], facts["name"]);
        let owners = block["declaring_classes"].as_array().unwrap();
        assert_eq!(owners.len(), methods.len());
        let mut mask = 0u64;
        for (i, (owner, method)) in owners.iter().zip(methods).enumerate() {
            let declaring_class = &data["declaring_classes"][owner.as_u64().unwrap() as usize];
            // Non-Fallable blocks take the same no-op/source defaults as
            // Fallable itself. Liquid interfaces keep their existing policy.
            let absent_fallable = method["base"] == "net.minecraft.world.level.block.Fallable"
                && declaring_class == "<not implemented>";
            if declaring_class != &method["base"] && !absent_fallable {
                mask |= 1 << i;
            }
        }
        writeln!(out, "{mask},").unwrap();
    }
    out.push_str("];\n");
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("block_methods.rs"),
        out,
    )
    .unwrap();
}
