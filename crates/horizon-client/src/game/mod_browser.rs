//! Capability `ui`'s panel beside container screens (WIT `interface
//! browser`, docs/mods.md): the items a recipe viewer mod lists, drawn as
//! the game's own item slots in pages right of the screen's window, with a
//! search box under them. The mod says what the panel holds; the game pages
//! it and tells the mod what the player clicked, typed, and which of the
//! mod's keys went down over an item. With no mod showing a panel, each
//! hook here returns after one check.
use super::Game;
#[cfg(feature = "mods")]
use horizon_client::ui::EditBox;

#[cfg(not(feature = "mods"))]
impl Game<'_> {
    pub(super) fn mod_browser_closed(&mut self) {}
}

/// The panel's state in the game.
#[cfg(feature = "mods")]
#[derive(Default)]
pub(super) struct Browser {
    search: Option<EditBox>,
}

#[cfg(feature = "mods")]
impl Game<'_> {
    /// The container screen closed: the search box lets go of the keys.
    pub(super) fn mod_browser_closed(&mut self) {
        if let Some(search) = self.mods.browser.search.as_mut() {
            search.focused = false;
        }
    }
}
