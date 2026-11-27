//! `WinScreen` in the game: the poem and credits after leaving the End for
//! the first time (`EndPortalBlock.entityInside` -> `ServerPlayer.showEndCredits`,
//! or a server's `WIN_GAME` event), and the credits from Credits &
//! Attribution.
use super::{Game, Screen};
use horizon_client::Result;

/// What closing the screen does (`WinScreen.onFinished`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Finish {
    /// `PERFORM_RESPAWN` in single player: the player, out of the End since
    /// the credits began, arrives in the Overworld.
    Respawn,
}
pub(super) struct Win {
    finish: Finish,
}
impl Game<'_> {
    pub(super) fn open_win_screen(&mut self, finish: Finish) {
        self.win = Some(Win { finish });
        self.keys.clear();
        self.screen = Screen::Win;
        self.capture(false);
    }
    /// `ServerPlayer.showEndCredits`: the player leaves the End now (no
    /// longer in any level until the credits end) and has seen the credits.
    pub(super) fn show_end_credits(&mut self) -> Result<()> {
        self.level.seen_credits = true;
        self.open_win_screen(Finish::Respawn);
        self.end_portal_travel = true;
        let result = self.enter_dimension(super::Dim::Overworld);
        self.end_portal_travel = false;
        result
    }
    /// The player waits out of the world while the poem rolls.
    pub(super) fn win_screen_holds_arrival(&self) -> bool {
        self.win
            .as_ref()
            .is_some_and(|w| w.finish == Finish::Respawn)
    }
}
