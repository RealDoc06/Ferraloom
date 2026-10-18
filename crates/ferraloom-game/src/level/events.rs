//! Port of ServerLevel's insertion-ordered, deduplicated block-event drain.
use super::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::Block;
use ferraloom_util::BlockPos;
use std::collections::{HashSet, VecDeque};
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockEvent {
    pub pos: BlockPos,
    pub block: Block,
    pub param_a: i32,
    pub param_b: i32,
}
#[derive(Default)]
pub(super) struct BlockEvents {
    queue: VecDeque<BlockEvent>,
    members: HashSet<BlockEvent>,
}
impl BlockEvents {
    fn insert(&mut self, event: BlockEvent) {
        if self.members.insert(event) {
            self.queue.push_back(event);
        }
    }
    fn pop(&mut self) -> Option<BlockEvent> {
        let event = self.queue.pop_front()?;
        self.members.remove(&event);
        Some(event)
    }
}
impl Level {
    pub fn block_event(&mut self, event: BlockEvent) {
        self.region_mut().block_events.insert(event);
    }
    /// Call after chunk ticking/broadcasts and before entity ticking. Events
    /// enqueued by callbacks run now; non-ticking events retain insertion order.
    pub fn run_block_events(&mut self) -> Result<(), LevelError> {
        let mut reschedule = Vec::new();
        while let Some(event) = self.region_mut().block_events.pop() {
            if !self.should_tick_blocks_at(event.pos) {
                reschedule.push(event);
                continue;
            }
            let state = self.get_block_state(event.pos)?;
            if state.block() == event.block
                && self
                    .required_block_behavior(event.block, crate::block::BlockMethod::TriggerEvent)?
                    .trigger_event(state, event.pos, event.param_a, event.param_b, self)?
            {
                self.emit(LevelEffect::BlockEvent(event));
            }
        }
        for event in reschedule {
            self.block_event(event);
        }
        Ok(())
    }
}
