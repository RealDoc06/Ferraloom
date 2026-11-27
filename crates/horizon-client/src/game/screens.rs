//! The pause menu (PauseScreen) and death screen (DeathScreen) layouts.
use super::{Game, Screen};
use horizon_client::{Result, command::args::GameMode};

impl Game<'_> {
    /// LocalPlayer.respawn: the player comes back (the death screen's
    /// button, or at once with `immediate_respawn`).
    pub(super) fn respawn_player(&mut self) -> Result<()> {
        {
            // PlayerList.respawn removes the dead player, and Player.remove's
            // inventoryMenu.removed drops what the cursor held, whatever
            // keep_inventory says (the 20-tick branch handles a later respawn).
            if let Some(stack) = self.carried.take()
                && let Some(eye) = self.eye_position()
            {
                self.throw_from_eyes(stack, eye, false);
            }
            let keep = self.ui.rules.bool("keep_inventory") || self.spectator();
            self.experience.respawn(keep);
            // The guests are sent the respawned host as a new entity, not the dead body.
            if let Some(host) = self.lan.as_mut() {
                let id = host.host_entity;
                host.hub.respawned(id);
            }
            // Autosaves keep a login position; death must resolve the respawn point instead.
            self.level.position = None;
        }
        if self.respawn_config.is_some() {
            // PlayerList.respawn at the respawn anchor, once it is loaded.
            self.begin_anchor_respawn()?;
            self.screen = Screen::Playing;
            self.ui.death_text = None;
            self.spawn_player()?;
            self.capture(true);
        } else {
            self.respawn_out_of_end()?;
            self.player = None;
            self.screen = Screen::Playing;
            self.ui.death_text = None;
            self.spawn_player()?;
            self.capture(true);
        }
        // ServerGamePacketListenerImpl.handleClientCommand: a hardcore world's
        // respawned player only watches.
        if self.level.hardcore {
            self.set_mode(GameMode::Spectator);
        }
        Ok(())
    }
}
