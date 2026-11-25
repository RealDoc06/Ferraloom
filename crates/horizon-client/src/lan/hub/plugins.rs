//! Ferraloom: Bukkit plugins' commands in every player's command tree
//! (docs/ferraloom/plugins.md). The game sets them (game/plugins.rs); they
//! follow vanilla's and the server mods'.
use super::Hub;
use serde_json::Value;
use std::borrow::Cow;

impl Hub {
    /// Plugins' commands changed (a root in the commands report's format,
    /// `ferraloom_plugins::command_tree`): every player gets their tree again.
    pub fn set_plugin_commands(&mut self, commands: Value) {
        if self.plugin_commands == commands {
            return;
        }
        self.plugin_commands = commands;
        for i in 0..self.guests.len() {
            if self.guests[i].placed {
                self.send_commands(i);
            }
        }
    }

    /// The roots after vanilla's: the server mods', then the plugins' commands
    /// a server mod does not already have.
    pub(super) fn extra_commands(&self) -> Cow<'_, Value> {
        let mods = &self.mod_commands.0;
        match (mods.is_null(), self.plugin_commands.is_null()) {
            (_, true) => Cow::Borrowed(mods),
            (true, false) => Cow::Borrowed(&self.plugin_commands),
            (false, false) => {
                let mut merged = mods.clone();
                if let (Some(into), Some(from)) = (
                    merged["children"].as_object_mut(),
                    self.plugin_commands["children"].as_object(),
                ) {
                    for (name, node) in from {
                        into.entry(name.clone()).or_insert_with(|| node.clone());
                    }
                }
                Cow::Owned(merged)
            }
        }
    }
}
