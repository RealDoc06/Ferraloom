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
        self.ensure_poi_chunk_loaded(ChunkPos::containing(update.pos()))?;
        match update {
            PoiUpdate::Remove { pos } => {
                self.region_mut().poi.remove(pos).map_err(LevelError::Poi)?;
            }
            PoiUpdate::Add { pos, poi_type } => {
                self.region_mut()
                    .poi
                    .add(pos, poi_type)
                    .map_err(LevelError::Poi)?;
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
        let old_type = PoiTypes::builtin().for_state(old);
        let new_type = PoiTypes::builtin().for_state(new);
        if old_type == new_type {
            return Ok(());
        }
        let actions = old_type
            .map(|_| PoiUpdate::Remove { pos })
            .into_iter()
            .chain(new_type.map(|poi_type| PoiUpdate::Add { pos, poi_type }));
        for action in actions {
            if self.region().poi_executor_running {
                self.emit(LevelEffect::PoiUpdate(action));
            } else {
                self.apply_poi_update(action)?;
            }
        }
        Ok(())
    }
}
