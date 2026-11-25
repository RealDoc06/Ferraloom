//! The players area: messages, locations and teleports; joins, quits,
//! chat and commands.
use super::{DIMS, GameHost, Pending, is_plugin_command, pend, world_index};
use crate::game::{Dim, Game, commands::Target, lan::LanHost};
use ferraloom_plugins::{Event, Joined, Location, PlayerHost, PlayerId, listens};
use horizon_client::{
    lan::{data::component_nbt, guest::Guest},
    text::{self, Text},
};
use horizon_protocol::packets::play::{self, Clientbound as C};

fn system_chat(line: &Text) -> C {
    C::SystemChat(play::SystemChat {
        content: component_nbt(&line.component()),
        overlay: false,
    })
}

/// A broadcast join or quit message (legacy text): vanilla's own translation
/// when the plugins left it as it was.
fn announcement(message: &str, default: &str, key: &str, name: &str) -> Text {
    if message == default {
        Text::translate(key, &[Text::literal(name.to_owned())]).with_color(text::YELLOW)
    } else {
        Text::legacy(message)
    }
}

impl GameHost<'_, '_> {
    fn guest(&mut self, id: PlayerId) -> Option<&Guest> {
        let hub = self.hub()?;
        hub.hub.guests.iter().find(|g| g.uuid == id && g.placed)
    }
}

impl PlayerHost for GameHost<'_, '_> {
    fn send_message(&mut self, player: PlayerId, message: &str) {
        let Some(hub) = self.hub() else {
            pend(Pending::Message(player, message.to_owned()));
            return;
        };
        let to = hub.hub.guests.iter().find(|g| g.uuid == player && g.placed);
        if let Some(conn) = to.map(|g| g.conn) {
            hub.hub.send_to(conn, &system_chat(&Text::legacy(message)));
        }
    }

    fn location(&mut self, player: PlayerId) -> Option<Location> {
        // The guest whose turn it is stands in the game's own player.
        if self.game.lan_acting_uuid == Some(player) {
            let p = self.game.player.as_ref()?;
            return Some(Location {
                world: world_index(self.game.dim),
                x: p.position.x,
                y: p.position.y,
                z: p.position.z,
                yaw: self.game.yaw,
                pitch: self.game.pitch,
            });
        }
        let g = self.guest(player)?;
        Some(Location {
            world: world_index(Dim::from_id(&g.dimension)),
            x: g.position[0],
            y: g.position[1],
            z: g.position[2],
            yaw: g.yaw,
            pitch: g.pitch,
        })
    }

    /// As `/tp` moves a player (across levels too). Another player than the
    /// one acting, while the hub is out of `Game::lan`, moves at the start of
    /// the next tick.
    fn teleport(&mut self, player: PlayerId, to: Location) -> bool {
        let Some(&level) = DIMS.get(to.world) else {
            return false;
        };
        let target = if self.game.lan_acting_uuid == Some(player) {
            Target::Player
        } else if !self.hub_in_game() {
            if self.guest(player).is_none() {
                return false;
            }
            pend(Pending::Teleport(player, to));
            return true;
        } else {
            match self.guest(player) {
                Some(g) => Target::Guest(g.conn),
                None => return false,
            }
        };
        let (pos, rotation) = ([to.x, to.y, to.z], Some([to.yaw, to.pitch]));
        if !self.game.teleport_across(target, level, pos, rotation) {
            self.game.teleport_to(target, pos, rotation);
        }
        true
    }
}

impl Game<'_> {
    /// PlayerList.placeNewPlayer's PlayerJoinEvent, once the guest is in the
    /// world. True when a listener chose the join message (the hub sent none;
    /// this sends the event's to every player, as CraftBukkit does).
    pub(in crate::game) fn plugin_join(&mut self, host: &mut LanHost, conn: u32) -> bool {
        if !ferraloom_plugins::running() {
            return false;
        }
        let Some(g) = host.hub.guest(conn).filter(|g| g.placed) else {
            return false;
        };
        let (id, name, entity_id, op) = (g.uuid, g.name.clone(), g.entity_id, g.permission > 0);
        let default = format!("\u{a7}e{name} joined the game");
        let joined = Joined {
            id,
            name: &name,
            entity_id,
            op,
            message: &default,
        };
        let message = ferraloom_plugins::player_join(&mut GameHost::new(self, Some(host)), &joined);
        if !listens(Event::PlayerJoin) {
            return false;
        }
        if let Some(message) = message {
            let line = announcement(&message, &default, "multiplayer.player.joined", &name);
            host.hub.broadcast(&system_chat(&line));
            crate::game::dedicated::log("INFO", &format!("System chat: {}", line.plain()));
        }
        true
    }

    /// PlayerList.remove's PlayerQuitEvent for a guest who left. True when a
    /// listener chose the quit message (sent here to everyone still on).
    pub(in crate::game) fn plugin_quit(&mut self, host: &mut LanHost, guest: &Guest) -> bool {
        if !ferraloom_plugins::running() || !guest.placed {
            return false;
        }
        let default = format!("\u{a7}e{} left the game", guest.name);
        let at = Location {
            world: world_index(Dim::from_id(&guest.dimension)),
            x: guest.position[0],
            y: guest.position[1],
            z: guest.position[2],
            yaw: guest.yaw,
            pitch: guest.pitch,
        };
        let mut gh = GameHost::new(self, Some(host));
        let message = ferraloom_plugins::player_quit(&mut gh, guest.uuid, &default, at);
        if !listens(Event::PlayerQuit) {
            return false;
        }
        if let Some(message) = message {
            let line = announcement(&message, &default, "multiplayer.player.left", &guest.name);
            host.hub.broadcast(&system_chat(&line));
            crate::game::dedicated::log("INFO", &format!("System chat: {}", line.plain()));
        }
        true
    }

    /// A player's chat line, which the hub left to AsyncPlayerChatEvent.
    pub(in crate::game) fn plugin_chat(&mut self, sender: u128, message: &str) {
        if listens(Event::AsyncPlayerChat) {
            ferraloom_plugins::chat(sender, message);
        }
    }

    /// A command a guest sent: true when a plugin command took it.
    pub(in crate::game) fn plugin_guest_command(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        line: &str,
    ) -> bool {
        if !ferraloom_plugins::running() || !is_plugin_command(line) {
            return false;
        }
        let Some(id) = host.hub.guest(conn).map(|g| g.uuid) else {
            return false;
        };
        ferraloom_plugins::player_command(&mut GameHost::new(self, Some(host)), id, line)
    }
}
