//! Method registration and region-owned state for the redstone family port.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::Block;

#[derive(Default)]
pub(crate) struct RedstoneState {}

pub(crate) fn ported_behavior(
    _block: Block,
    _method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    None
}
