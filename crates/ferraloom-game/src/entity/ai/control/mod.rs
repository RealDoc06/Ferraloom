//! Live control boundaries, replaced individually as their native bodies land.
use super::{AiOperation, pending};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
};

/// Port of MoveControl#tick's live entry point; implementation pending.
pub fn tick_move(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    pending(entity, AiOperation::MoveControlTick, level)
}
/// Port of LookControl#tick's live entry point; implementation pending.
pub fn tick_look(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    pending(entity, AiOperation::LookControl, level)
}
/// Port of JumpControl#tick's live entry point; implementation pending.
pub fn tick_jump(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    pending(entity, AiOperation::JumpControl, level)
}
