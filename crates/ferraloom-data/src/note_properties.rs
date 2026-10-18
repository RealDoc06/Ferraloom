//! Extracted BlockBehaviour instrument and NoteBlockInstrument parameters.
use crate::{Block, properties::Instrument, registries::sound_event::Id as SoundEvent};
#[derive(Clone, Copy, Debug)]
pub struct NoteInstrumentProperties {
    pub sound: SoundEvent,
    pub tunable: bool,
    pub custom_sound: bool,
    pub works_above: bool,
}
include!(concat!(env!("OUT_DIR"), "/note_properties.rs"));
impl Block {
    /// Port of BlockState#instrument, a constructor fact shared by its states.
    pub const fn note_block_instrument(self) -> Instrument {
        BLOCK_INSTRUMENTS[self.protocol_id() as usize]
    }
}
