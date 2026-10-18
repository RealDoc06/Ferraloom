//! Immutable constructor parameters used by redstone sources and opening blocks.
use std::{fmt::Write, path::Path};
pub fn generate(root: &Path) {
    let data = super::load(root, "redstone_properties");
    let mut out = String::from("const REDSTONE_PROPERTIES: &[RedstoneProperties] = &[\n");
    for (id, block) in data.as_array().unwrap().iter().enumerate() {
        assert_eq!(block["id"].as_u64().unwrap(), id as u64);
        write!(
            out,
            "RedstoneProperties {{ button_ticks: {}, max_weight: {}, block_set: ",
            block["button_ticks"], block["max_weight"]
        )
        .unwrap();
        let set = &block["set"];
        if set.is_null() {
            out.push_str("None,");
        } else {
            out.push_str("Some(BlockSetProperties {");
            for key in [
                "can_open_by_hand",
                "arrows_press_button",
                "mobs_press_plate",
            ] {
                write!(out, "{key}: {},", set[key]).unwrap();
            }
            for key in [
                "door_open",
                "door_close",
                "trapdoor_open",
                "trapdoor_close",
                "button_on",
                "button_off",
                "plate_on",
                "plate_off",
            ] {
                write!(
                    out,
                    "{key}: crate::registries::sound_event::Id({}),",
                    set[key]
                )
                .unwrap();
            }
            out.push_str("}),");
        }
        for key in ["gate_open", "gate_close"] {
            if block[key].is_null() {
                write!(out, "{key}: None,").unwrap();
            } else {
                write!(
                    out,
                    "{key}: Some(crate::registries::sound_event::Id({})),",
                    block[key]
                )
                .unwrap();
            }
        }
        out.push_str("},\n");
    }
    out.push_str("];\n");
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("redstone_properties.rs"),
        out,
    )
    .unwrap();
}
