//! Live sensing boundary; the native Mob-owned cache adapter is pending.
use super::{AiOperation, pending};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
};

/// Port of Sensing#tick's live entry point; implementation pending.
pub fn tick(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    pending(entity, AiOperation::Sensing, level)
}
