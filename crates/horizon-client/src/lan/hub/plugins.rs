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

    /// One player's command tree again, as their plugin permissions now
    /// filter it (they joined; `Player.updateCommands`). False when they are
    /// not placed.
    pub fn resend_plugin_commands(&self, player: u128) -> bool {
        let i = self
            .guests
            .iter()
            .position(|g| g.uuid == player && g.placed);
        if let Some(i) = i {
            self.send_commands(i);
        }
        i.is_some()
    }

    /// [`Self::extra_commands`] for one player: without the plugin commands
    /// they may not use (CraftBukkit's nodes require `testPermissionSilent`).
    /// Asks the plugins once per command tree sent, only while they have commands.
    pub(super) fn extra_commands_for(&self, player: u128) -> Cow<'_, Value> {
        let extra = self.extra_commands();
        if self.plugin_commands.is_null() {
            return extra;
        }
        let hidden = ferraloom_plugins::hidden_commands(player);
        if hidden.is_empty() {
            return extra;
        }
        let mut tree = extra.into_owned();
        if let Some(children) = tree["children"].as_object_mut() {
            for label in &hidden {
                // A server mod's command of the same name stays.
                if self.mod_commands.0["children"].get(label).is_none() {
                    children.remove(label);
                }
            }
        }
        Cow::Owned(tree)
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
