//! Ferraloom: what the plugin event sites in this library share
//! ([`crate::plugin_hooks`]). Each site tests its hook first and calls
//! here only when a plugin listens, so these never run without plugins.
use super::{LevelAccess, Sim};
use crate::plugin_hooks::PluginEvent;
use horizon_core::{geometry::Direction, position::BlockPos};

impl<L: LevelAccess + ?Sized> Sim<'_, L> {
    /// Fires `event` to the game's plugins: true when cancelled.
    #[cold]
    pub(crate) fn plugin_cancels(&mut self, event: PluginEvent) -> bool {
        self.level.plugin_event(event) != 0
    }

    /// `BlockPistonExtendEvent`/`BlockPistonRetractEvent` with the blocks
    /// that move, then those that break (CraftBukkit's list): true when cancelled.
    #[cold]
    pub(crate) fn plugin_piston_cancels(
        &mut self,
        pos: BlockPos,
        direction: Direction,
        extending: bool,
        push: &[BlockPos],
        destroy: &[BlockPos],
    ) -> bool {
        let moved: Vec<BlockPos> = push.iter().chain(destroy).copied().collect();
        self.plugin_cancels(PluginEvent::Piston {
            pos,
            direction,
            extending,
            moved: &moved,
        })
    }
}
