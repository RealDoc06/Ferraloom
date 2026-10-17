use ferraloom_data::BlockState;
use ferraloom_util::BlockPos;

use crate::level::{Level, LevelError};

/// Port of BlockBehaviour's scheduled-tick virtual method.
///
/// One immutable object per vanilla block class; state is always registry data.
pub trait BlockBehavior: Sync {
    fn tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut dyn Level,
    ) -> Result<(), LevelError>;
}
