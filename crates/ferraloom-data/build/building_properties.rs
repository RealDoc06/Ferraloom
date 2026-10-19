//! Pure native state predicates and constructor parameters used by placement.
use std::{fmt::Write, path::Path};
pub fn generate(root: &Path) {
    let data = super::load(root, "building_properties");
    let states = data["state_count"].as_u64().unwrap() as usize;
    let blocks = data["block_count"].as_u64().unwrap() as usize;
    let mut burn = vec![0_u64; states.div_ceil(64)];
    let mut targets = vec![u16::MAX; blocks];
    for state in data["can_burn"].as_array().unwrap() {
        let id = state.as_u64().unwrap() as usize;
        burn[id / 64] |= 1 << (id % 64);
    }
    for pair in data["concrete"].as_array().unwrap() {
        targets[pair[0].as_u64().unwrap() as usize] = pair[1].as_u64().unwrap().try_into().unwrap();
    }
    let mut out = String::new();
    writeln!(out, "const FIRE_CAN_BURN: &[u64] = &{burn:?};").unwrap();
    writeln!(out, "const CONCRETE_POWDER_TARGETS: &[u16] = &{targets:?};").unwrap();
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("building_properties.rs"),
        out,
    )
    .unwrap();
}
