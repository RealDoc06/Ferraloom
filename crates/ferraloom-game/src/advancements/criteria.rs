//! Typed CriteriaTriggers seam. The advancements port will evaluate listeners
//! here. Until then each trigger is a whole effect under UnportedBehavior;
//! skipping a trigger never substitutes a predicate result or skips its caller.
use crate::{
    block::UnportedOperation,
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_protocol::Identifier;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Trigger {
    GenerateLoot,
}
impl Trigger {
    pub const fn name(self) -> &'static str {
        match self {
            Self::GenerateLoot => "generate_loot",
        }
    }
}

/// Port of CriteriaTriggers.GENERATE_LOOT.trigger, before the table is cleared.
pub fn generate_loot(
    level: &mut Level,
    _player: EntityRef,
    _table: &Identifier,
) -> Result<(), LevelError> {
    level.unported_operation(UnportedOperation::AdvancementCriterion(
        Trigger::GenerateLoot,
    ))
}
