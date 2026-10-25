use crate::Block;

include!(concat!(env!("OUT_DIR"), "/block_methods.rs"));

impl Block {
    /// Whether the concrete vanilla block class overrides the base implementation
    /// of this exact signature. Bases live in BlockBehaviour, except placement,
    /// stepping, falling and bouncing methods whose vanilla base lives in Block;
    /// liquid interfaces use SimpleWaterloggedBlock's concrete defaults.
    /// Fallable defaults also apply to non-implementing blocks.
    pub const fn overrides(self, method: BlockMethod) -> bool {
        BLOCK_METHOD_OVERRIDES[self.protocol_id() as usize] & (1 << method as u8) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_and_overridden_vanilla_methods_keep_their_identity() {
        assert!(!Block::STONE.overrides(BlockMethod::HandlePrecipitation));
        assert!(Block::CAULDRON.overrides(BlockMethod::HandlePrecipitation));
        assert!(!Block::STONE.overrides(BlockMethod::CanSurvive));
        assert!(Block::OAK_SAPLING.overrides(BlockMethod::CanSurvive));
        assert!(!Block::AIR.overrides(BlockMethod::UpdateShape));
        assert!(Block::OAK_LEAVES.overrides(BlockMethod::UpdateShape));
        assert!(!Block::STONE.overrides(BlockMethod::Rotate));
        assert!(Block::OAK_LOG.overrides(BlockMethod::Rotate));
        assert!(!Block::STONE.overrides(BlockMethod::GetStateForPlacement));
        assert!(Block::OAK_LOG.overrides(BlockMethod::GetStateForPlacement));
        assert!(!Block::STONE.overrides(BlockMethod::SetPlacedBy));
        assert!(Block::OAK_DOOR.overrides(BlockMethod::SetPlacedBy));
        assert!(Block::REPEATER.overrides(BlockMethod::SetPlacedBy));
        assert!(!Block::STONE.overrides(BlockMethod::PlayerWillDestroy));
        assert!(Block::TRIPWIRE.overrides(BlockMethod::PlayerWillDestroy));
        assert!(!Block::REDSTONE_WIRE.overrides(BlockMethod::SpawnDestroyByEntityParticles));
        assert!(Block::FIRE.overrides(BlockMethod::SpawnDestroyByEntityParticles));
        assert!(!Block::FIRE.overrides(BlockMethod::SpawnDestroyParticles));
        assert!(!Block::STONE.overrides(BlockMethod::SpawnDestroyParticles));
        assert!(Block::STRAW_BED.overrides(BlockMethod::SpawnDestroyParticles));
        assert!(!Block::STONE.overrides(BlockMethod::OnLand));
        assert!(!Block::SAND.overrides(BlockMethod::OnLand));
        assert!(!Block::STONE.overrides(BlockMethod::GetFallDamageSource));
        assert!(!Block::SAND.overrides(BlockMethod::GetFallDamageSource));
        assert!(Block::ANVIL.overrides(BlockMethod::OnLand));
        assert!(Block::DAMAGED_ANVIL.overrides(BlockMethod::OnBrokenAfterFall));
        assert!(Block::CHIPPED_ANVIL.overrides(BlockMethod::GetFallDamageSource));
        assert!(Block::WHITE_CONCRETE_POWDER.overrides(BlockMethod::OnLand));
        assert!(!Block::WHITE_CONCRETE_POWDER.overrides(BlockMethod::GetFallDamageSource));
        assert!(Block::SUSPICIOUS_SAND.overrides(BlockMethod::OnBrokenAfterFall));
        assert!(Block::POINTED_DRIPSTONE.overrides(BlockMethod::OnBrokenAfterFall));
        assert!(Block::SULFUR_SPIKE.overrides(BlockMethod::GetFallDamageSource));
    }
}
