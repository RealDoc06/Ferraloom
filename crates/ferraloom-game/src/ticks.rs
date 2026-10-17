use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet},
};

use ferraloom_data::Block;
use ferraloom_util::{BlockPos, ChunkPos};

/// Port of TickPriority, in Java enum order.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum TickPriority {
    ExtremelyHigh,
    VeryHigh,
    High,
    #[default]
    Normal,
    Low,
    VeryLow,
    ExtremelyLow,
}

/// Port of ScheduledTick<Block>. Equality/heap keys below deliberately differ
/// from the uniqueness key, which is only (position, block).
#[derive(Clone, Copy, Debug)]
pub struct ScheduledTick {
    pub pos: BlockPos,
    pub block: Block,
    pub trigger_tick: i64,
    pub priority: TickPriority,
    pub sub_tick_order: i64,
}

#[derive(Default)]
struct ChunkTicks {
    queue: BTreeMap<(i64, TickPriority, i64), ScheduledTick>,
    positions: HashSet<(BlockPos, Block)>,
}

impl ChunkTicks {
    fn peek(&self) -> Option<ScheduledTick> {
        self.queue.first_key_value().map(|(_, tick)| *tick)
    }
    fn pop(&mut self) -> ScheduledTick {
        let (_, tick) = self.queue.pop_first().expect("nonempty chunk tick queue");
        self.positions.remove(&(tick.pos, tick.block));
        tick
    }
}

/// Port of LevelTicks / LevelChunkTicks. No hash iteration determines tick order.
#[derive(Default)]
pub struct LevelTicks {
    containers: HashMap<ChunkPos, ChunkTicks>,
    next_order: i64,
}

impl LevelTicks {
    pub fn schedule(
        &mut self,
        pos: BlockPos,
        block: Block,
        trigger_tick: i64,
        priority: TickPriority,
    ) {
        let order = self.next_order;
        self.next_order = self
            .next_order
            .checked_add(1)
            .expect("scheduled tick order exhausted");
        let container = self
            .containers
            .entry(ChunkPos::containing(pos))
            .or_default();
        if !container.positions.insert((pos, block)) {
            return;
        }
        container.queue.insert(
            (trigger_tick, priority, order),
            ScheduledTick {
                pos,
                block,
                trigger_tick,
                priority,
                sub_tick_order: order,
            },
        );
    }

    /// LevelTicks compares due container heads by priority/order, not due time.
    /// Per-container heads still use time/priority/order. Collecting before calling
    /// game code prevents same-phase recursion from newly scheduled zero-delay work.
    pub fn collect_due(&mut self, game_time: i64, budget: usize) -> Vec<ScheduledTick> {
        let mut ready = BinaryHeap::new();
        for (&chunk, container) in &self.containers {
            if let Some(head) = container.peek()
                && head.trigger_tick <= game_time
            {
                ready.push(Reverse((head.priority, head.sub_tick_order, chunk.pack())));
            }
        }
        let mut due = Vec::new();
        while due.len() < budget {
            let Some(Reverse((_, _, chunk))) = ready.pop() else {
                break;
            };
            let container = self
                .containers
                .get_mut(&ChunkPos::unpack(chunk))
                .expect("selected container");
            due.push(container.pop());
            if let Some(head) = container.peek()
                && head.trigger_tick <= game_time
            {
                ready.push(Reverse((head.priority, head.sub_tick_order, chunk)));
            }
        }
        due
    }
}
