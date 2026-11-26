//! `BundleMouseActions`: the wheel over a bundle in a container screen selects
//! one of the items it shows, which a click that takes one out of it then takes
//! (`BundleContents.Mutable.removeOne`). Moving off the bundle clears the
//! selection. On a server the selection is sent to it (`ServerboundSelectBundleItemPacket`).
use super::{Game, Screen};
use horizon_client::bundle::toggle_selected;

#[derive(Default)]
pub(super) struct BundleMouse {
    /// The slot under the cursor at the last tick (`AbstractContainerScreen.hoveredSlot`).
    hovered: Option<usize>,
}

impl Game<'_> {
    /// A slot a bundle can be selected in: the player's inventory, the off hand and a
    /// container's own slots. A server only knows these in the inventory and chest-like menus.
    fn bundle_slot(&mut self, slot: usize) -> bool {
        (slot < 36 || slot == 504 || (300..500).contains(&slot))
            && (slot < 36 || slot == 504 || self.slots().contains(&slot))
    }

    /// `BundleMouseActions.toggleSelectedBundleItem`: the stack here and, on a server, there.
    fn select_bundle(&mut self, slot: usize, index: i32) {
        if let Some(stack) = self.slot_mut(slot) {
            toggle_selected(stack, index);
        }
    }

    /// `BundleMouseActions.unselectedBundleItem` for the bundle in `slot`, if it has a selection.
    pub(super) fn unselect_bundle(&mut self, slot: usize) {
        if !self.bundle_slot(slot) {
            return;
        }
        let selected = self
            .slot_mut(slot)
            .as_ref()
            .and_then(|s| s.data())
            .is_some_and(|d| d.bundle_selected.0 >= 0);
        if selected {
            self.select_bundle(slot, -1);
        }
    }

    /// `AbstractContainerScreen.onStopHovering` (and `onClose`): leaving a slot, or the
    /// screen, clears the bundle's selection.
    pub(super) fn tick_bundle_hover(&mut self) {
        let now = if self.screen == Screen::Inventory {
            self.inventory_target().0
        } else {
            None
        };
        if let Some(left) = std::mem::replace(&mut self.bundle_mouse.hovered, now)
            && Some(left) != now
        {
            self.unselect_bundle(left);
        }
    }
}
