//! Live navigation boundary over the shared pathfinder; implementation pending.
use super::{AiOperation, pending};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
};

/// Port of PathNavigation#tick's live entry point; implementation pending.
pub fn tick(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    pending(entity, AiOperation::NavigationTick, level)
}
