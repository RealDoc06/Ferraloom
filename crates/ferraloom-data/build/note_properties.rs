//! Immutable note instrument facts, kept out of gameplay dispatch.
use std::{fmt::Write, path::Path};
pub fn generate(root: &Path) {
    let data = super::load(root, "note_properties");
    let mut out = String::from("const BLOCK_INSTRUMENTS: &[crate::properties::Instrument] = &[\n");
    for name in data["blocks"].as_array().unwrap() {
        writeln!(
            out,
            "crate::properties::Instrument::{},",
            super::pascal(name.as_str().unwrap())
        )
        .unwrap();
    }
    out.push_str("];\nimpl crate::properties::Instrument {\n/// Constructor parameters of vanilla's NoteBlockInstrument.\npub const fn note_properties(self) -> NoteInstrumentProperties { match self {\n");
    for entry in data["instruments"].as_array().unwrap() {
        writeln!(out,"Self::{} => NoteInstrumentProperties {{ sound: crate::registries::sound_event::Id::from_protocol_id({}).unwrap(), tunable: {}, custom_sound: {}, works_above: {} }},",super::pascal(entry["name"].as_str().unwrap()),entry["sound"],entry["tunable"],entry["custom"],entry["above"]).unwrap();
    }
    out.push_str("} } }\n");
    std::fs::write(
        std::env::var("OUT_DIR").unwrap() + "/note_properties.rs",
        out,
    )
    .unwrap();
}
