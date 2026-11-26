//! A server's dialogs bound to the game (`ClientCommonPacketListenerImpl.
//! showDialog` and `clearDialog`, `DialogScreen`, its `WarningScreen` and the
//! `WaitingForResponseScreen`): the screen over whatever is open (the
//! connecting screen in configuration), its inputs, and the actions its
//! buttons send back.
use super::Game;
use std::time::Instant;

/// An open `DialogScreen`.
struct Open {}

/// The dialog screens a server can open.
#[derive(Default)]
pub(super) struct DialogState {
    open: Option<Open>,
    /// `WaitingForResponseScreen`, since when.
    waiting: Option<Instant>,
    /// `DialogScreen.WarningScreen`, over the dialog.
    warning: bool,
}

impl Game<'_> {
    /// Whether a dialog screen takes the input.
    pub(super) fn dialog_open(&self) -> bool {
        let d = &self.dialogs;
        d.open.is_some() || d.waiting.is_some() || d.warning
    }
}
