//! The game-mode switch a guest's F3+F4 asks the server for
//! (`ServerboundChangeGameModePacket`).
use super::Game;
use horizon_client::{command::args::GameMode, text::Text};

impl Game<'_> {
    /// `ServerboundChangeGameModePacket` handled by the integrated server:
    /// `GameModeCommand.setGameMode` with its feedback.
    pub(super) fn change_game_mode(&mut self, mode: GameMode) {
        if self.set_game_mode(mode) {
            let name = Text::translate(&format!("gameMode.{}", mode.name()), &[]);
            self.feedback(Text::translate("commands.gamemode.success.self", &[name]));
        }
    }
}
