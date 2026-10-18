//! Vanilla redstone block family. Each virtual method is registered separately;
//! unimplemented overrides continue through the required-behavior error path.
use super::{BlockBehavior, BlockMethod};
use ferraloom_data::{Block, BlockClass};

pub mod signal;
pub mod piston;
mod state;
pub mod comparator;
pub mod storage;

/// Region-local counterpart of mutable redstone block state in the Java
/// registry. Dispatchers remain stateless across reentrant level callbacks.
#[derive(Default)]
pub struct RedstoneState {
    pub(crate) wire_signals_suppressed: bool,
}

pub(crate) fn ported_behavior(block: Block, method: BlockMethod) -> Option<&'static dyn BlockBehavior> {
    use BlockMethod as M;
    match block.class() {
        BlockClass::PoweredBlock if matches!(method, M::IsSignalSource | M::OwnSignal) => Some(&signal::PoweredBlock),
        _ => None,
    }
}
