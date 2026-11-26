//! The mouse handling every survival and container screen shares
//! (`AbstractContainerScreen.mouseClicked` / `mouseReleased`): which slot id
//! a click targets (-999 outside the window, `hasClickedOutside` with the
//! recipe book's own rule) and which `ContainerInput` it sends. Shift is
//! the event's own modifiers (`InputWithModifiers.hasShiftDown`: the
//! physical Shift keys, never the remapped or toggled sneak key).

/// The left, right and middle buttons (SDL's 1, 3 and 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// The input state `mouseClicked`/`mouseReleased` keep between events.
#[derive(Clone, Debug, Default)]
pub struct Clicks {
    /// `isQuickCrafting` with `quickCraftingButton`.
    quick_crafting: Option<Button>,
    skip_next_release: bool,
}

impl Clicks {
    /// The screen that takes the pointer over starts with its own state:
    /// `AbstractContainerScreen` holds `isQuickCrafting` and the drag it
    /// belongs to on the screen, so a screen that opens (or closes) drops
    /// them. A press whose release never arrived -- the pointer left the
    /// window -- would otherwise keep the drag running, and `press` answers
    /// `None` for every later press, so the quick move and the clicks go
    /// dead on every screen after it.
    pub fn cancel_drag(&mut self) {
        self.quick_crafting = None;
        self.skip_next_release = false;
    }
}
