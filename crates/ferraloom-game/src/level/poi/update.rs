use super::{PoiTypeId, PoiTypes};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::BlockState;
use ferraloom_util::{BlockPos, ChunkPos};

/// ServerLevel#updatePOIOnBlockStateChange captures removal then addition as
/// distinct server.execute tasks, so each action retains its original receiver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoiUpdate {
    Remove { pos: BlockPos },
    Add { pos: BlockPos, poi_type: PoiTypeId },
}
impl PoiUpdate {
    /// Capture ServerLevel#updatePOIOnBlockStateChange's distinct remove/add tasks.
    /// This reads immutable type data only and never loads or mutates a chunk.
    pub fn for_block_change(pos: BlockPos, old: BlockState, new: BlockState) -> [Option<Self>; 2] {
        let old_type = PoiTypes::builtin().for_state(old);
        let new_type = PoiTypes::builtin().for_state(new);
        if old_type == new_type {
            return [None, None];
        }
        [
            old_type.map(|_| Self::Remove { pos }),
            new_type.map(|poi_type| Self::Add { pos, poi_type }),
        ]
    }

    pub fn pos(self) -> BlockPos {
        match self {
            Self::Remove { pos } | Self::Add { pos, .. } => pos,
        }
    }
}
impl Level {
    /// Input from ReentrantBlockableEventLoop's task boundary. Return the prior
    /// value so nested owner tasks restore their caller's execution context.
    pub fn set_poi_executor_running(&mut self, running: bool) -> bool {
        std::mem::replace(&mut self.region_mut().poi_executor_running, running)
    }
    pub fn apply_poi_update(&mut self, update: PoiUpdate) -> Result<(), LevelError> {
        self.apply_poi_update_shared(update)
    }
    /// Execute one already-dequeued main task, including inside managedBlock.
    /// POI-only IO finishes before the short canonical manager guard is acquired;
    /// neither branch dispatches block/entity callbacks or invokes caller code.
    pub fn apply_poi_update_shared(&self, update: PoiUpdate) -> Result<(), LevelError> {
        self.ensure_poi_chunk_loaded_shared(ChunkPos::containing(update.pos()))?;
        let mut manager = self.region().poi.borrow_mut();
        match update {
            PoiUpdate::Remove { pos } => {
                manager.remove(pos).map_err(LevelError::Poi)?;
            }
            PoiUpdate::Add { pos, poi_type } => {
                manager.add(pos, poi_type).map_err(LevelError::Poi)?;
            }
        }
        Ok(())
    }
    /// Port of ServerLevel#updatePOIOnBlockStateChange. Outside a running task,
    /// owner-thread execute is synchronous; reentrant main tasks queue actions.
    /// Production setBlock wiring waits for the ordered POI IO continuation.
    pub fn update_poi_on_block_state_change(
        &mut self,
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    ) -> Result<(), LevelError> {
        let actions = PoiUpdate::for_block_change(pos, old, new)
            .into_iter()
            .flatten();
        for action in actions {
            if self.region().poi_executor_running {
                self.emit(LevelEffect::PoiUpdate(action));
            } else {
                self.apply_poi_update(action)?;
            }
        }
        Ok(())
    }
    /// WorldGenRegion#setBlock submits these captured changes to server.execute.
    /// Transport replays its global write journal here; execution remains on the
    /// main-task queue even when transport completes outside a running task.
    pub fn enqueue_poi_block_state_change(
        &mut self,
        pos: BlockPos,
        old: BlockState,
        new: BlockState,
    ) {
        for action in PoiUpdate::for_block_change(pos, old, new)
            .into_iter()
            .flatten()
        {
            self.emit(LevelEffect::PoiUpdate(action));
        }
    }
}
