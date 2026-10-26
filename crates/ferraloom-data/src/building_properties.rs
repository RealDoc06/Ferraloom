//! Bootstrapped native facts; live neighbor and tag queries stay in game.
use crate::{Block, BlockState};
include!(concat!(env!("OUT_DIR"), "/building_properties.rs"));
impl BlockState {
    /// Port of FireBlock#getIgniteOdds, including the waterlogged-state guard.
    pub const fn fire_ignite_odds(self) -> u8 {
        FIRE_IGNITE_ODDS[self.id() as usize]
    }
    /// Port of FireBlock#getBurnOdds, including the waterlogged-state guard.
    pub const fn fire_burn_odds(self) -> u8 {
        FIRE_BURN_ODDS[self.id() as usize]
    }
    /// Port of FireBlock#canBurn for this exact state, including waterlogging.
    pub const fn can_burn_in_fire(self) -> bool {
        let id = self.id() as usize;
        FIRE_CAN_BURN[id / 64] & (1 << (id % 64)) != 0
    }
}
impl Block {
    /// ConcretePowderBlock's constructor-supplied target, for every color.
    pub fn concrete_powder_target(self) -> Option<Block> {
        Block::from_protocol_id(CONCRETE_POWDER_TARGETS[self.protocol_id() as usize])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockClass, properties as p};
    #[test]
    fn native_fire_state_predicates_and_all_powder_targets_are_complete() {
        for id in 0..crate::BLOCK_STATES.len() {
            let state = BlockState::from_id(id as u16).unwrap();
            assert_eq!(state.can_burn_in_fire(), state.fire_ignite_odds() > 0);
        }
        assert_eq!(Block::OAK_PLANKS.default_state().fire_ignite_odds(), 5);
        assert_eq!(Block::OAK_PLANKS.default_state().fire_burn_odds(), 20);
        assert_eq!(Block::WHITE_WOOL.default_state().fire_ignite_odds(), 30);
        assert_eq!(Block::WHITE_WOOL.default_state().fire_burn_odds(), 60);
        assert!(Block::OAK_FENCE.default_state().can_burn_in_fire());
        assert!(
            !Block::OAK_FENCE
                .default_state()
                .with(p::WATERLOGGED, true)
                .unwrap()
                .can_burn_in_fire()
        );
        let wet_fence = Block::OAK_FENCE
            .default_state()
            .with(p::WATERLOGGED, true)
            .unwrap();
        assert_eq!(
            (wet_fence.fire_ignite_odds(), wet_fence.fire_burn_odds()),
            (0, 0)
        );
        assert!(!Block::NETHER_BRICK_FENCE.default_state().can_burn_in_fire());
        let mut powders = 0;
        for &block in crate::registries::block::ALL {
            assert_eq!(
                block.concrete_powder_target().is_some(),
                block.class() == BlockClass::ConcretePowderBlock
            );
            if let Some(target) = block.concrete_powder_target() {
                assert_ne!(target, block);
                powders += 1;
            }
        }
        assert_eq!(powders, 16);
        assert_eq!(
            Block::WHITE_CONCRETE_POWDER.concrete_powder_target(),
            Some(Block::WHITE_CONCRETE)
        );
    }
}
