//! Source-proven immutable redstone block constructor parameters.
use crate::{Block, registries::sound_event::Id as SoundEvent};
#[derive(Clone, Copy, Debug)]
pub struct BlockSetProperties {
    pub can_open_by_hand: bool,
    pub arrows_press_button: bool,
    pub mobs_press_plate: bool,
    pub door_open: SoundEvent,
    pub door_close: SoundEvent,
    pub trapdoor_open: SoundEvent,
    pub trapdoor_close: SoundEvent,
    pub button_on: SoundEvent,
    pub button_off: SoundEvent,
    pub plate_on: SoundEvent,
    pub plate_off: SoundEvent,
}
#[derive(Clone, Copy, Debug)]
pub struct RedstoneProperties {
    pub button_ticks: i32,
    pub max_weight: i32,
    pub block_set: Option<BlockSetProperties>,
    pub gate_open: Option<SoundEvent>,
    pub gate_close: Option<SoundEvent>,
}
include!(concat!(env!("OUT_DIR"), "/redstone_properties.rs"));
impl Block {
    pub const fn redstone_properties(self) -> &'static RedstoneProperties {
        &REDSTONE_PROPERTIES[self.protocol_id() as usize]
    }
}
