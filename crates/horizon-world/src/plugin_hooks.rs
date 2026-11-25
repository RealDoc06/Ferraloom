//! Ferraloom: the plugin events vanilla's block behaviour fires
//! (docs/ferraloom/plugins.md, "blocks"). This crate knows nothing of
//! plugins; the game copies which of these events have listeners into
//! [`set`] between ticks, and answers [`PluginEvent`]s through
//! [`LevelAccess::plugin_event`](crate::level::LevelAccess::plugin_event).
//!
//! A site is
//!
//! ```ignore
//! if plugin_hooks::listens(Hook::Burn) && self.level.plugin_event(PluginEvent::Burn { .. }) != 0 {
//!     return;
//! }
//! ```
//!
//! so without a listener it is one relaxed load and a branch never taken:
//! no call, direct or indirect, and no event value built.
use horizon_core::{geometry::Direction, position::BlockPos};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::registry::BlockStateId;

/// The events, as bits of [`listens`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Hook {
    Physics,
    FromTo,
    Burn,
    Ignite,
    Grow,
    Fade,
    Form,
    Spread,
    LeavesDecay,
    PistonExtend,
    PistonRetract,
    Redstone,
}

impl Hook {
    const fn bit(self) -> u32 {
        1 << self as u8
    }
}

/// The bits of the hooks with listeners; 0 without plugins.
static LISTENING: AtomicU32 = AtomicU32::new(0);

/// Whether a plugin listens to `hook` this tick.
#[inline(always)]
pub fn listens(hook: Hook) -> bool {
    LISTENING.load(Ordering::Relaxed) & hook.bit() != 0
}

/// Latches the hooks with listeners, between ticks.
pub fn set(hooks: impl IntoIterator<Item = Hook>) {
    let bits = hooks.into_iter().fold(0, |m, h| m | h.bit());
    LISTENING.store(bits, Ordering::Relaxed);
}

/// Why fire lit a block (`BlockIgniteEvent.IgniteCause`, in its order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum IgniteCause {
    Lava = 0,
    FlintAndSteel = 1,
    Spread = 2,
    Lightning = 3,
    Fireball = 4,
    EnderCrystal = 5,
    Explosion = 6,
    Arrow = 7,
}

/// An event a site fires, with what the event needs. Each names its CraftBukkit site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PluginEvent<'a> {
    /// `BlockPhysicsEvent` (NeighborUpdater.executeUpdate), before `neighborChanged` runs.
    Physics {
        pos: BlockPos,
        state: BlockStateId,
        source: BlockPos,
    },
    /// `BlockFromToEvent` (FlowingFluid.spreadTo), before a fluid flows from `from` to `to`.
    FromTo {
        from: BlockPos,
        to: BlockPos,
        face: Direction,
    },
    /// `BlockBurnEvent` (FireBlock.checkBurnOut), before fire destroys `pos`.
    Burn { pos: BlockPos, fire: BlockPos },
    /// `BlockIgniteEvent` (FireBlock.tick's spread, LavaFluid.randomTick...), before `pos` is lit.
    Ignite {
        pos: BlockPos,
        cause: IgniteCause,
        source: Option<BlockPos>,
    },
    /// `BlockGrowEvent` (handleBlockGrowEvent), before `pos` becomes `to`.
    Grow { pos: BlockPos, to: BlockStateId },
    /// `BlockFadeEvent` (callBlockFadeEvent), before `pos` becomes `to`.
    Fade { pos: BlockPos, to: BlockStateId },
    /// `BlockFormEvent` (handleBlockFormEvent), before `pos` becomes `to`.
    Form { pos: BlockPos, to: BlockStateId },
    /// `BlockSpreadEvent` (handleBlockSpreadEvent), before `pos` becomes `to`, spread from `source`.
    Spread {
        pos: BlockPos,
        source: BlockPos,
        to: BlockStateId,
    },
    /// `LeavesDecayEvent` (LeavesBlock.randomTick), before the leaves at `pos` drop and go.
    LeavesDecay { pos: BlockPos },
    /// `BlockPistonExtendEvent`/`BlockPistonRetractEvent` (PistonBaseBlock.moveBlocks),
    /// before anything moves; `moved` names the blocks that will.
    Piston {
        pos: BlockPos,
        direction: Direction,
        extending: bool,
        moved: &'a [BlockPos],
    },
    /// `BlockRedstoneEvent` (RedStoneWireBlock.updatePowerStrength): the answer is the power to use.
    Redstone { pos: BlockPos, old: i32, new: i32 },
}

impl PluginEvent<'_> {
    /// The hook a site tests before building this event.
    pub fn hook(&self) -> Hook {
        match self {
            Self::Physics { .. } => Hook::Physics,
            Self::FromTo { .. } => Hook::FromTo,
            Self::Burn { .. } => Hook::Burn,
            Self::Ignite { .. } => Hook::Ignite,
            Self::Grow { .. } => Hook::Grow,
            Self::Fade { .. } => Hook::Fade,
            Self::Form { .. } => Hook::Form,
            Self::Spread { .. } => Hook::Spread,
            Self::LeavesDecay { .. } => Hook::LeavesDecay,
            Self::Piston {
                extending: true, ..
            } => Hook::PistonExtend,
            Self::Piston { .. } => Hook::PistonRetract,
            Self::Redstone { .. } => Hook::Redstone,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_hooks_set_listen() {
        set([Hook::Burn, Hook::PistonRetract]);
        assert!(listens(Hook::Burn) && listens(Hook::PistonRetract));
        assert!(!listens(Hook::Physics) && !listens(Hook::PistonExtend));
        set([]);
        assert!(!listens(Hook::Burn));
    }
}
