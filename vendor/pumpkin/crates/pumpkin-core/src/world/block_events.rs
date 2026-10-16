use std::collections::{HashSet, VecDeque};

use pumpkin_util::math::position::BlockPos;

use crate::block::BlockEvent;

/// Pending events deduplicate in first-insertion order, with constant-time removal.
#[derive(Default)]
pub(super) struct BlockEventQueue {
    events: VecDeque<BlockEvent>,
    pending: HashSet<(BlockPos, u8, u8)>,
}

impl BlockEventQueue {
    pub(super) fn push(&mut self, event: BlockEvent) {
        if self.pending.insert((event.pos, event.r#type, event.data)) {
            self.events.push_back(event);
        }
    }

    pub(super) fn pop(&mut self) -> Option<BlockEvent> {
        let event = self.events.pop_front()?;
        self.pending.remove(&(event.pos, event.r#type, event.data));
        Some(event)
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(&BlockEvent) -> bool) {
        self.events.retain(|event| {
            let retained = keep(event);
            if !retained {
                self.pending.remove(&(event.pos, event.r#type, event.data));
            }
            retained
        });
    }
}
