use crate::Block;

include!(concat!(env!("OUT_DIR"), "/block_methods.rs"));

impl Block {
    /// Whether the concrete vanilla block class overrides the base implementation
    /// of this exact signature. Bases live in BlockBehaviour, except placement,
    /// stepping, falling and bouncing methods whose vanilla base lives in Block;
    /// liquid interfaces use SimpleWaterloggedBlock's concrete defaults.
    pub const fn overrides(self, method: BlockMethod) -> bool {
        BLOCK_METHOD_OVERRIDES[self.protocol_id() as usize] & (1 << method as u8) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_and_overridden_vanilla_methods_keep_their_identity() {
        assert!(!Block::STONE.overrides(BlockMethod::CanSurvive));
        assert!(Block::OAK_SAPLING.overrides(BlockMethod::CanSurvive));
        assert!(!Block::AIR.overrides(BlockMethod::UpdateShape));
        assert!(Block::OAK_LEAVES.overrides(BlockMethod::UpdateShape));
        assert!(!Block::STONE.overrides(BlockMethod::Rotate));
        assert!(Block::OAK_LOG.overrides(BlockMethod::Rotate));
        assert!(!Block::STONE.overrides(BlockMethod::GetStateForPlacement));
        assert!(Block::OAK_LOG.overrides(BlockMethod::GetStateForPlacement));
    }
}
