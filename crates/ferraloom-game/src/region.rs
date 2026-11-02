//! The value moved between the coordinator and a regional phase adapter.
use horizon_world::level::Ticks;

/// Stable simulation ordinal, never a worker index or completion sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u64);
impl RegionId {
    pub const REFERENCE: Self = Self(0);
}

/// `W` owns the actual resident world and its world-specific local buffers.
/// Gameplay resources and serial services belong to the adapter, not here.
/// The original queues and position RNG move intact with this value.
pub struct Region<W> {
    pub id: RegionId,
    pub world: W,
    pub ticks: Ticks,
}
impl<W> Region<W> {
    pub fn new(id: RegionId, world: W, ticks: Ticks) -> Self {
        Self { id, world, ticks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::{OwnershipError, OwnershipSlot};
    use horizon_core::position::BlockPos;

    #[test]
    fn ownership_handoff_preserves_tick_state_and_live_callback_mutations() {
        let mut ticks = Ticks::new(12345);
        assert_eq!(ticks.next_sub_tick(), 0);
        let mut slot = OwnershipSlot::new(Region::new(RegionId::REFERENCE, vec![1], ticks));
        let mut region = slot.take().unwrap();
        assert!(matches!(slot.as_ref(), Err(OwnershipError::LentToRegion)));
        // The exclusive callback receives the actual value, including queues
        // and the dimension-wide sub-tick sequence, and returns that owner.
        region.world.push(2);
        assert_eq!(region.ticks.next_sub_tick(), 1);
        let pos = BlockPos::new(1, 64, -1);
        region.ticks.comparator_outputs.insert(pos, 7);
        region.ticks.torch_toggles.push((pos, 42));
        region.ticks.random_tick_value = -5;
        slot.restore(region)
            .unwrap_or_else(|(error, _)| panic!("{error}"));
        assert_eq!(slot.id, RegionId::REFERENCE);
        assert_eq!(slot.world, [1, 2]);
        assert_eq!(slot.ticks.next_sub_tick(), 2);
        assert_eq!(slot.ticks.comparator_outputs[&pos], 7);
        assert_eq!(slot.ticks.torch_toggles, [(pos, 42)]);
        assert_eq!(slot.ticks.random_tick_value, -5);
    }
}
