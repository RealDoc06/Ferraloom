//! PistonMovingBlockEntity's canonical progress and persistent state.
use crate::block::entity::{
    BlockEntity, BlockEntityData, BlockEntityDataView, BlockEntityRef, BlockEntityState,
};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{BlockPos, Direction, mth};

/// Port of PistonMovingBlockEntity. Persisted progress is progressO, including
/// the half-tick visual/movement state at the moment a chunk is saved.
#[derive(Clone, Debug)]
pub struct PistonMovingBlockEntity {
    pub parent: BlockEntityData,
    pub moved_state: BlockState,
    pub direction: Direction,
    pub extending: bool,
    pub is_source_piston: bool,
    pub progress: f32,
    pub progress_o: f32,
    pub last_ticked: i64,
    pub death_ticks: i32,
}
impl PistonMovingBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            moved_state: Block::AIR.default_state(),
            direction: Direction::Down,
            extending: false,
            is_source_piston: false,
            progress: 0.0,
            progress_o: 0.0,
            last_ticked: 0,
            death_ticks: 0,
        }
    }
    pub fn moving(
        parent: BlockEntityData,
        moved_state: BlockState,
        direction: Direction,
        extending: bool,
        is_source_piston: bool,
    ) -> Self {
        Self {
            moved_state,
            direction,
            extending,
            is_source_piston,
            ..Self::new(parent)
        }
    }
    pub fn get_progress(&self, partial: f32) -> f32 {
        mth::lerp_f32(
            if partial > 1.0 { 1.0 } else { partial },
            self.progress_o,
            self.progress,
        )
    }
    pub fn extended_progress(&self, progress: f32) -> f32 {
        if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        }
    }
    pub fn movement_direction(&self) -> Direction {
        if self.extending {
            self.direction
        } else {
            self.direction.opposite()
        }
    }
    pub fn offset(&self, partial: f32) -> ferraloom_util::Vec3 {
        let progress = self.extended_progress(self.get_progress(partial));
        let step = self.direction.step();
        ferraloom_util::Vec3::new(
            f64::from(step.x as f32 * progress),
            f64::from(step.y as f32 * progress),
            f64::from(step.z as f32 * progress),
        )
    }
    pub fn position(&self) -> BlockPos {
        self.parent.pos
    }
}

pub struct PistonMovingEntityBehavior;
pub static MOVING_PISTON_ENTITY: PistonMovingEntityBehavior = PistonMovingEntityBehavior;
impl BlockEntity for PistonMovingEntityBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        if let Some(e) = PistonMovingBlockEntity::view_mut(state) {
            e.moved_state = tag
                .get("blockState")
                .and_then(crate::entity::persistence::load_block_state)
                .unwrap_or(Block::AIR.default_state());
            e.direction = Direction::from_3d_data_value(tag.get_int_or("facing", 0));
            e.progress = tag.get_float_or("progress", 0.0);
            e.progress_o = e.progress;
            e.extending = tag.get_boolean_or("extending", false);
            e.is_source_piston = tag.get_boolean_or("source", false);
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let e = PistonMovingBlockEntity::view(state).expect("moving piston dispatcher");
        let mut tag = CompoundTag::new();
        tag.insert(
            "blockState",
            crate::entity::persistence::save_block_state(e.moved_state),
        );
        tag.insert("facing", Tag::Int(e.direction as i32));
        tag.insert("progress", Tag::Float(e.progress_o));
        tag.insert("extending", Tag::Byte(i8::from(e.extending)));
        tag.insert("source", Tag::Byte(i8::from(e.is_source_piston)));
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, _entity: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(Block::MOVING_PISTON))
    }
    fn pre_remove_side_effects(
        &self,
        _entity: BlockEntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedBlockEntity(Block::MOVING_PISTON))
    }
}
