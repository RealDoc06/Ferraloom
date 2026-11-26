//! Capability `input` (docs/mods.md): the use button on items a mod claims,
//! the mod's own keys and the mouse wheel; and `render`'s zoom. With no mod
//! claiming items, keys or the wheel, each hook here returns after one
//! check.
use super::{Game, Screen};
use horizon_mods::HeldItem;

/// A claimed item's use while the button is down.
pub(super) struct ModUse {
    item: HeldItem,
    /// `Game::ticks` when it started.
    since: u64,
}

impl Game<'_> {
    /// The main hand's stack as `input` shows it; none when it is empty.
    pub(super) fn mod_held(&self) -> Option<HeldItem> {
        let stack = self.inventory.get(self.selected)?.as_ref()?;
        Some(HeldItem {
            id: stack.item.to_string(),
            count: stack.count,
            slot: self.selected as u32,
        })
    }
    /// Ends a claimed use: the button came up, the player switched slots,
    /// or a screen opened. False when none was going.
    pub(super) fn stop_mod_use(&mut self) -> bool {
        let Some(u) = self.mods.using.take() else {
            return false;
        };
        let ticks = self.ticks.saturating_sub(u.since).min(u64::from(u32::MAX)) as u32;
        self.mods.mods.use_stop(&u.item, ticks);
        true
    }
    /// Once a tick: a claimed use stops when its item leaves the hand or a
    /// screen opens, as vanilla's `releaseUsingItem`.
    pub(super) fn tick_mod_use(&mut self) {
        let Some(u) = &self.mods.using else {
            return;
        };
        let still = self
            .mod_held()
            .is_some_and(|h| h.id == u.item.id && h.slot == u.item.slot);
        if !still || self.screen != Screen::Playing {
            self.stop_mod_use();
        }
    }
}
