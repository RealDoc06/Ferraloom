//! Immutable native CopperChestBlock constructor facts. Membership in
//! COPPER_CHESTS remains a live datapack tag, independent of these class facts.
use crate::Block;

#[derive(Clone, Copy, Debug)]
pub struct CopperChestProperties {
    pub weathering_stage: u8,
    pub is_waxed: bool,
}
include!(concat!(env!("OUT_DIR"), "/copper_chest_properties.rs"));

impl Block {
    pub const fn copper_chest_properties(self) -> Option<&'static CopperChestProperties> {
        COPPER_CHEST_PROPERTIES[self.protocol_id() as usize].as_ref()
    }
}
