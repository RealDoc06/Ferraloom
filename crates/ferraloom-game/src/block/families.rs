//! One registration point per ported family. Each family declares precisely
//! which overridden virtual methods it implements; inherited bases stay central.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::Block;

pub(super) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    super::plants::ported_behavior(block, method)
        .or_else(|| super::redstone::ported_behavior(block, method))
        .or_else(|| super::containers::ported_behavior(block, method))
        .or_else(|| super::breaking::ported_behavior(block, method))
        .or_else(|| super::building::ported_behavior(block, method))
}
