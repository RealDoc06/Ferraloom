//! Plant-family registration. The family owner replaces this explicit pending
//! stub with method-specific ports shared by live and worldgen dispatch.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::Block;

pub fn ported_behavior(_block: Block, _method: BlockMethod) -> Option<&'static dyn BlockBehavior> {
    None
}
