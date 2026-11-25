//! The server area: the console, vanilla commands, shutdown.
use super::{GameHost, Pending, STOP, is_plugin_command, pend};
use crate::game::Game;
use ferraloom_plugins::ServerHost;
use horizon_client::text::Text;
use std::sync::atomic::Ordering;

impl ServerHost for GameHost<'_, '_> {
    fn console_message(&mut self, message: &str) {
        crate::game::dedicated::log("INFO", &Text::legacy(message).plain());
    }

    /// The console's line through the game's dispatcher, as the dedicated
    /// console runs it; while the hub is out, at the next tick's start.
    fn dispatch_vanilla(&mut self, line: &str) -> bool {
        if !self.hub_in_game() {
            pend(Pending::Vanilla(line.to_owned()));
            return true;
        }
        run_vanilla(self.game, line)
    }

    fn shutdown(&mut self) {
        STOP.store(true, Ordering::Relaxed);
    }
}

/// A console line through the game's dispatcher, its replies in the log.
pub(super) fn run_vanilla(game: &mut Game, line: &str) -> bool {
    game.chat_capture = Some(Vec::new());
    let ran = game.as_console(|g| g.run_command(line));
    for text in game.chat_capture.take().unwrap_or_default() {
        crate::game::dedicated::log("INFO", &format!("System chat: {}", text.plain()));
    }
    ran
}

impl Game<'_> {
    /// A console line: true when a plugin command took it.
    pub(in crate::game) fn plugin_console_command(&mut self, line: &str) -> bool {
        if !ferraloom_plugins::running() || !is_plugin_command(line) {
            return false;
        }
        ferraloom_plugins::console_command(&mut GameHost::new(self, None), line)
    }
}
