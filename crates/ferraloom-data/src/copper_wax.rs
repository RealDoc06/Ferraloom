//! Immutable HoneycombItem.WAXABLES and its native inverse. These are pure
//! registry facts: no level, tags, position, RNG or entity participates.
use crate::{Block, BlockState};
include!(concat!(env!("OUT_DIR"), "/copper_wax.rs"));

impl Block {
    /// Port of HoneycombItem.WAXABLES lookup.
    pub const fn waxed(self) -> Option<Block> {
        COPPER_WAXED[self.protocol_id() as usize]
    }
    /// Port of HoneycombItem.WAX_OFF_BY_BLOCK lookup, derived from WAXABLES.
    pub const fn unwaxed(self) -> Option<Block> {
        COPPER_UNWAXED[self.protocol_id() as usize]
    }
}
impl BlockState {
    /// Port of HoneycombItem#getWaxed, retaining every shared state property.
    pub fn waxed(self) -> Option<BlockState> {
        self.block()
            .waxed()
            .map(|block| block.default_state().with_properties_of(self))
    }
    /// Native wax-off state projection, sharing the same extracted bijection.
    pub fn unwaxed(self) -> Option<BlockState> {
        self.block()
            .unwaxed()
            .map(|block| block.default_state().with_properties_of(self))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_copper_wax_pairs_are_bijective() {
        let mut pairs = 0;
        for id in 0..crate::BLOCKS.len() {
            let block = Block(id as u16);
            if let Some(waxed) = block.waxed() {
                assert_eq!(waxed.unwaxed(), Some(block));
                assert_eq!(block.unwaxed(), None);
                assert_eq!(waxed.waxed(), None);
                pairs += 1;
            }
        }
        assert_eq!(pairs, 60);
        assert_eq!(Block::COPPER_BLOCK.waxed(), Some(Block::WAXED_COPPER_BLOCK));
        assert_eq!(Block::STONE.waxed(), None);
    }
}
