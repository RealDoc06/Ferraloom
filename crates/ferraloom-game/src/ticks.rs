use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
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
    pub sub_tick_order: u64,
}

#[derive(Default)]
struct ChunkTicks {
    // Unique sub-tick order makes payload comparison unnecessary.
    queue: BinaryHeap<Reverse<(i64, TickPriority, u64)>>,
    payloads: HashMap<u64, ScheduledTick>,
    positions: HashSet<(BlockPos, Block)>,
}

impl ChunkTicks {
    fn peek(&self) -> Option<ScheduledTick> {
        self.queue
            .peek()
            .map(|Reverse((_, _, order))| self.payloads[order])
    }
    fn pop(&mut self) -> ScheduledTick {
        let Reverse((_, _, order)) = self.queue.pop().expect("nonempty chunk tick queue");
        let tick = self.payloads.remove(&order).expect("queued tick payload");
        self.positions.remove(&(tick.pos, tick.block));
        tick
    }
}

/// Port of LevelTicks / LevelChunkTicks. No hash iteration determines tick order.
#[derive(Default)]
pub struct LevelTicks {
    containers: HashMap<ChunkPos, ChunkTicks>,
    next_order: u64,
}

impl LevelTicks {
    pub fn schedule(
        &mut self,
        pos: BlockPos,
        block: Block,
        trigger_tick: i64,
        priority: TickPriority,
    ) {
        let container = self
            .containers
            .entry(ChunkPos::containing(pos))
            .or_default();
        if !container.positions.insert((pos, block)) {
            return;
        }
        let order = self.next_order;
        self.next_order = self
            .next_order
            .checked_add(1)
            .expect("scheduled tick order exhausted");
        container.payloads.insert(
            order,
            ScheduledTick {
                pos,
                block,
                trigger_tick,
                priority,
                sub_tick_order: order,
            },
        );
        container
            .queue
            .push(Reverse((trigger_tick, priority, order)));
    }

    /// LevelTicks compares due container heads by priority/order, not due time.
    /// Per-container heads still use time/priority/order. Collecting before calling
    /// game code prevents same-phase recursion from newly scheduled zero-delay work.
    pub fn collect_due(&mut self, game_time: i64, budget: usize) -> Vec<ScheduledTick> {
        let mut due = Vec::new();
        while due.len() < budget {
            let next = self
                .containers
                .iter()
                .filter_map(|(&chunk, container)| {
                    let head = container.peek()?;
                    (head.trigger_tick <= game_time).then_some((chunk, head))
                })
                .min_by_key(|(_, tick)| (tick.priority, tick.sub_tick_order));
            let Some((chunk, _)) = next else {
                break;
            };
            due.push(
                self.containers
                    .get_mut(&chunk)
                    .expect("selected container")
                    .pop(),
            );
        }
        due
    }
}
