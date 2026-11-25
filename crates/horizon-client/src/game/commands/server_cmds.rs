//! The commands only a dedicated server has (horizon-command's
//! `tree_server.rs`): operators, bans, the whitelist, saving, stopping, the
//! idle timeout and transfers. The console and the players run the same
//! code; what it changes is the server's `Admin` (game/dedicated.rs).
use super::{Game, R, Target, err};
use crate::game::dedicated::{self, Lists};
use horizon_client::{
    command::{Arg, Context},
    server::user_lists::{BanEntry, NameAndId, OpEntry, WhiteEntry},
    text::{self, Text},
};
use horizon_protocol::{
    packets::{
        configuration::Transfer,
        play::{self, Clientbound as C},
    },
    text::translatable,
};
use std::sync::{Arc, Mutex};

/// A player's `/save-all`: during their turn their state stands in for the
/// server's, which `save_game` would write as the world's, so the save
/// waits for the turn to end (`run_deferred_saves`). What vanilla tells
/// once the save is done is made in the turn and sent then.
pub(in crate::game) struct DeferredSave {
    conn: u32,
    lines: Vec<Text>,
    admin: Vec<(Text, bool, bool)>,
}

/// ComponentUtils.formatList with the gray `, ` separator.
fn format_list(items: Vec<Text>) -> Text {
    if items.len() == 1 {
        return items.into_iter().next().unwrap_or_default();
    }
    let mut out = Text::empty();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(Text::literal(", ").with_color(text::GRAY));
        }
        out.push(item);
    }
    out
}

/// BanListEntry.getReasonMessage.
fn reason_message(reason: &Option<String>) -> Text {
    match reason {
        Some(r) => Text::literal(r.clone()),
        None => Text::translate("multiplayer.disconnect.banned.reason.default", &[]),
    }
}

impl Game<'_> {
    /// The `server.*` commands (one arm of `execute_command`).
    pub(super) fn cmd_server(&mut self, id: &str, ctx: &Context) -> R<i32> {
        let Some(lists) = dedicated::admin(|a| a.lists.clone()) else {
            return Err(err("command.failed", &[]));
        };
        match id {
            "server.op" => self.cmd_op(&lists, ctx),
            "server.deop" => self.cmd_deop(&lists, ctx),
            "server.ban" => self.cmd_ban(&lists, ctx),
            "server.ban-ip" => self.cmd_ban_ip(&lists, ctx),
            "server.pardon" => self.cmd_pardon(&lists, ctx),
            "server.pardon-ip" => self.cmd_pardon_ip(&lists, ctx),
            "server.banlist" => self.cmd_banlist(&lists, ctx),
            "server.save-all" => self.cmd_save_all(),
            "server.save-off" | "server.save-on" => self.cmd_save_toggle(id == "server.save-on"),
            "server.setidletimeout" => self.cmd_setidletimeout(ctx),
            "server.stop" => {
                // StopCommand: MinecraftServer.halt; the loop ends after this tick.
                self.feedback(Text::translate("commands.stop.stopping", &[]));
                dedicated::admin(|a| a.stop = true);
                Ok(1)
            }
            "server.transfer" => self.cmd_transfer(ctx),
            id => self.cmd_whitelist(&lists, id, ctx),
        }
    }

    /// GameProfileArgument.getGameProfiles: a name is looked up (the user
    /// cache, then the profile service); a selector finds online players.
    fn profiles(&mut self, ctx: &Context) -> R<Vec<NameAndId>> {
        if let Some(Arg::Str(name)) = ctx.get("targets") {
            let name = name.clone();
            return self
                .resolve(&name)
                .map(|u| vec![u])
                .ok_or_else(|| err("argument.player.unknown", &[]));
        }
        let found = self.entities(ctx, "targets", true)?;
        Ok(found
            .into_iter()
            .filter_map(|t| self.profile_of(t))
            .collect())
    }

    /// Player.nameAndId of a player target.
    fn profile_of(&self, t: Target) -> Option<NameAndId> {
        match t {
            Target::Player => Some(NameAndId {
                id: self.lan_acting_uuid?,
                name: self.acting_player_name().to_owned(),
            }),
            Target::Guest(conn) => self.command_guest(conn).map(|g| NameAndId {
                id: g.uuid,
                name: g.name.clone(),
            }),
            _ => None,
        }
    }

    /// CommandSourceStack.getTextName: who a ban names as its source.
    fn source_text_name(&self) -> String {
        if self.command_source.is_some_and(|s| s.block.is_some()) {
            return "@".into();
        }
        match self.source_entity() {
            Some(t) => self.target_name(t).plain(),
            None => "Server".into(),
        }
    }

    /// Sends the operators what the command told them so far. Vanilla
    /// broadcasts before it disconnects anyone, so a banned operator still
    /// hears of their ban: every kick here comes after this.
    fn flush_admin_feedback(&mut self) {
        if let Some(mut host) = self.lan.take() {
            self.send_admin_feedback(&mut host, self.lan_acting);
            self.lan = Some(host);
        }
    }

    /// MessageArgument.getMessage(...).getString(): the optional reason.
    fn reason(&mut self, ctx: &Context) -> Option<String> {
        match ctx.get("reason") {
            Some(Arg::Str(raw)) => {
                let raw = raw.clone();
                Some(self.expand_selectors(&raw).plain())
            }
            _ => None,
        }
    }

    /// Port of OpCommand.opPlayers (PlayerList.op at `op-permission-level`).
    fn cmd_op(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let level = dedicated::admin(|a| a.op_level).unwrap_or(4);
        let mut count = 0;
        for user in self.profiles(ctx)? {
            let added = {
                let mut l = lists.lock().unwrap();
                !l.ops.contains(&user)
                    && l.ops.add(OpEntry {
                        user: user.clone(),
                        level,
                        bypasses_player_limit: false,
                    })
            };
            if added {
                count += 1;
                self.sync_ops(lists);
                self.feedback(Text::translate(
                    "commands.op.success",
                    &[Text::literal(user.name)],
                ));
            }
        }
        if count == 0 {
            return Err(err("commands.op.failed", &[]));
        }
        Ok(count)
    }

    /// Port of DeOpCommands.deopPlayers.
    fn cmd_deop(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let mut count = 0;
        for user in self.profiles(ctx)? {
            if lists.lock().unwrap().ops.remove(&user) {
                count += 1;
                self.sync_ops(lists);
                self.feedback(Text::translate(
                    "commands.deop.success",
                    &[Text::literal(user.name)],
                ));
            }
        }
        if count == 0 {
            return Err(err("commands.deop.failed", &[]));
        }
        self.flush_admin_feedback();
        self.kick_unlisted_players(lists);
        Ok(count)
    }

    /// Port of BanPlayerCommands.banPlayers.
    fn cmd_ban(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let reason = self.reason(ctx);
        let source = self.source_text_name();
        let mut count = 0;
        for user in self.profiles(ctx)? {
            let added = {
                let mut l = lists.lock().unwrap();
                !l.bans.contains(&user)
                    && l.bans.add(BanEntry::new(
                        user.clone(),
                        Some(source.clone()),
                        None,
                        reason.clone(),
                    ))
            };
            if added {
                count += 1;
                self.feedback(Text::translate(
                    "commands.ban.success",
                    &[Text::literal(user.name), reason_message(&reason)],
                ));
                self.flush_admin_feedback();
                self.kick_uuid(user.id, translatable("multiplayer.disconnect.banned", &[]));
            }
        }
        if count == 0 {
            return Err(err("commands.ban.failed", &[]));
        }
        Ok(count)
    }

    /// Port of BanIpCommands.banIpOrName and banIp.
    fn cmd_ban_ip(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let Some(Arg::Str(target)) = ctx.get("target") else {
            return Err(err("commands.banip.invalid", &[]));
        };
        let target = target.clone();
        let ip = if target.parse::<std::net::IpAddr>().is_ok() {
            target
        } else {
            // PlayerList.getPlayerByName, then ServerPlayer.getIpAddress.
            self.lan
                .as_ref()
                .and_then(|h| {
                    h.hub
                        .guests
                        .iter()
                        .find(|g| g.name.eq_ignore_ascii_case(&target))
                })
                .map(|g| g.address.ip().to_string())
                .ok_or_else(|| err("commands.banip.invalid", &[]))?
        };
        if lists.lock().unwrap().ip_bans.contains(&ip) {
            return Err(err("commands.banip.failed", &[]));
        }
        // PlayerList.getPlayersWithAddress.
        let players: Vec<(u128, u32)> = self
            .lan
            .iter()
            .flat_map(|h| h.hub.guests.iter())
            .filter(|g| g.address.ip().to_string() == ip)
            .map(|g| (g.uuid, g.conn))
            .collect();
        let reason = self.reason(ctx);
        lists.lock().unwrap().ip_bans.add(BanEntry::new(
            ip.clone(),
            Some(self.source_text_name()),
            None,
            reason.clone(),
        ));
        self.feedback(Text::translate(
            "commands.banip.success",
            &[Text::literal(ip), reason_message(&reason)],
        ));
        if !players.is_empty() {
            let names = players
                .iter()
                .map(|&(_, conn)| {
                    let t = match self.lan_acting {
                        Some(acting) if acting == conn => Target::Player,
                        _ => Target::Guest(conn),
                    };
                    self.target_name(t)
                })
                .collect();
            self.feedback(Text::translate(
                "commands.banip.info",
                &[Text::literal(players.len().to_string()), format_list(names)],
            ));
        }
        self.flush_admin_feedback();
        for &(uuid, _) in &players {
            self.kick_uuid(uuid, translatable("multiplayer.disconnect.ip_banned", &[]));
        }
        Ok(players.len() as i32)
    }

    /// Port of PardonCommand.pardonPlayers.
    fn cmd_pardon(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let mut count = 0;
        for user in self.profiles(ctx)? {
            if lists.lock().unwrap().bans.remove(&user) {
                count += 1;
                self.feedback(Text::translate(
                    "commands.pardon.success",
                    &[Text::literal(user.name)],
                ));
            }
        }
        if count == 0 {
            return Err(err("commands.pardon.failed", &[]));
        }
        Ok(count)
    }

    /// Port of PardonIpCommand.unban.
    fn cmd_pardon_ip(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let Some(Arg::Str(ip)) = ctx.get("target") else {
            return Err(err("commands.pardonip.invalid", &[]));
        };
        let ip = ip.clone();
        if ip.parse::<std::net::IpAddr>().is_err() {
            return Err(err("commands.pardonip.invalid", &[]));
        }
        if !lists.lock().unwrap().ip_bans.remove(&ip) {
            return Err(err("commands.pardonip.failed", &[]));
        }
        self.feedback(Text::translate(
            "commands.pardonip.success",
            &[Text::literal(ip)],
        ));
        Ok(1)
    }

    /// Port of BanListCommands.showList: the player bans, then the IP bans.
    fn cmd_banlist(&mut self, lists: &Arc<Mutex<Lists>>, ctx: &Context) -> R<i32> {
        let entries: Vec<(String, String, Option<String>)> = {
            let l = lists.lock().unwrap();
            let players = l
                .bans
                .entries()
                .into_iter()
                .map(|b| (b.user.name.clone(), b.source.clone(), b.reason.clone()));
            let ips = l
                .ip_bans
                .entries()
                .into_iter()
                .map(|b| (b.user.clone(), b.source.clone(), b.reason.clone()));
            match (ctx.has("ips"), ctx.has("players")) {
                (true, _) => ips.collect(),
                (_, true) => players.collect(),
                _ => players.chain(ips).collect(),
            }
        };
        if entries.is_empty() {
            self.feedback(Text::translate("commands.banlist.none", &[]));
            return Ok(0);
        }
        self.feedback(Text::tr("commands.banlist.list", &[&entries.len()]));
        let count = entries.len() as i32;
        for (name, source, reason) in entries {
            self.feedback(Text::translate(
                "commands.banlist.entry",
                &[
                    Text::literal(name),
                    Text::literal(source),
                    reason_message(&reason),
                ],
            ));
        }
        Ok(count)
    }

    /// Port of SaveAllCommand.saveAll (`flush` saves the same: Horizon's
    /// save always waits for the chunks to be written).
    fn cmd_save_all(&mut self) -> R<i32> {
        self.feedback(Text::translate("commands.save.saving", &[]));
        let Some(conn) = self.lan_acting else {
            return match self.save_game() {
                Ok(()) => {
                    self.feedback(Text::translate("commands.save.success", &[]));
                    Ok(1)
                }
                Err(e) => {
                    dedicated::log("ERROR", &format!("{e}"));
                    Err(err("commands.save.failed", &[]))
                }
            };
        };
        // A player's turn: what "Saved the game" sends is made now and held
        // until the save has run.
        let lines = self.chat_capture.as_ref().map_or(0, Vec::len);
        let admin = self.admin_feedback.len();
        self.feedback(Text::translate("commands.save.success", &[]));
        let save = DeferredSave {
            conn,
            lines: self
                .chat_capture
                .as_mut()
                .map(|c| c.split_off(lines))
                .unwrap_or_default(),
            admin: self.admin_feedback.split_off(admin),
        };
        dedicated::admin(|a| a.saves.push(save));
        Ok(1)
    }

    /// The players' `/save-all` once the tick's turns are over: the save,
    /// then the success (or the failure) to the player and the operators.
    pub(in crate::game) fn run_deferred_saves(&mut self) {
        let saves = dedicated::admin(|a| std::mem::take(&mut a.saves)).unwrap_or_default();
        for save in saves {
            let saved = self.save_game();
            let Some(mut host) = self.lan.take() else {
                return;
            };
            let ok = saved.is_ok();
            let lines = match saved {
                Ok(()) => save.lines,
                Err(e) => {
                    dedicated::log("ERROR", &format!("{e}"));
                    let failed = Text::translate("commands.save.failed", &[]);
                    vec![Text::empty().append(failed).with_color(text::RED)]
                }
            };
            for line in lines {
                host.hub.send_to(
                    save.conn,
                    &C::SystemChat(play::SystemChat {
                        content: horizon_client::lan::data::component_nbt(&line.component()),
                        overlay: false,
                    }),
                );
            }
            if ok {
                self.admin_feedback = save.admin;
                self.send_admin_feedback(&mut host, Some(save.conn));
            }
            self.lan = Some(host);
        }
    }

    /// Port of SaveOffCommand and SaveOnCommand (MinecraftServer.setAutoSave).
    fn cmd_save_toggle(&mut self, on: bool) -> R<i32> {
        let changed =
            dedicated::admin(|a| std::mem::replace(&mut a.autosave, on) != on).unwrap_or(false);
        match (changed, on) {
            (false, true) => Err(err("commands.save.alreadyOn", &[])),
            (false, false) => Err(err("commands.save.alreadyOff", &[])),
            (true, true) => {
                self.feedback(Text::translate("commands.save.enabled", &[]));
                Ok(1)
            }
            (true, false) => {
                self.feedback(Text::translate("commands.save.disabled", &[]));
                Ok(1)
            }
        }
    }

    /// Port of SetPlayerIdleTimeoutCommand (DedicatedServer.setPlayerIdleTimeout
    /// stores it in server.properties).
    fn cmd_setidletimeout(&mut self, ctx: &Context) -> R<i32> {
        let minutes = Self::int(ctx, "minutes").unwrap_or(0);
        dedicated::admin(|a| a.idle_timeout = minutes);
        self.store_property("player-idle-timeout", minutes.to_string());
        self.feedback(if minutes > 0 {
            Text::tr("commands.setidletimeout.success", &[&minutes])
        } else {
            Text::translate("commands.setidletimeout.success.disabled", &[])
        });
        Ok(minutes)
    }

    /// Port of TransferCommand.transfer: each player is sent to `hostname`.
    fn cmd_transfer(&mut self, ctx: &Context) -> R<i32> {
        let Some(Arg::Str(host)) = ctx.get("hostname") else {
            return Err(err("command.failed", &[]));
        };
        let host = host.clone();
        let port = Self::int(ctx, "port").unwrap_or(25565);
        let players = if ctx.get("players").is_some() {
            self.entities(ctx, "players", true)?
        } else {
            // CommandSourceStack.getPlayerOrException.
            let me = self
                .source_entity()
                .filter(|t| t.is_player())
                .ok_or_else(|| err("permissions.requires.player", &[]))?;
            vec![me]
        };
        for &t in &players {
            let conn = match t {
                Target::Guest(conn) => Some(conn),
                _ => self.lan_acting,
            };
            if let (Some(conn), Some(h)) = (conn, self.lan.as_ref()) {
                h.hub.send_to(
                    conn,
                    &C::Transfer(Transfer {
                        host: host.clone(),
                        port,
                    }),
                );
            }
        }
        // CommandResponseTracker: the one player by name, else how many.
        let message = match players.as_slice() {
            [] => return Err(err("commands.transfer.error.no_players", &[])),
            [one] => Text::translate(
                "commands.transfer.success.single",
                &[
                    self.target_name(*one),
                    Text::literal(host),
                    Text::literal(port.to_string()),
                ],
            ),
            many => Text::translate(
                "commands.transfer.success.multiple",
                &[
                    Text::literal(many.len().to_string()),
                    Text::literal(host),
                    Text::literal(port.to_string()),
                ],
            ),
        };
        self.feedback(message);
        Ok(players.len() as i32)
    }

    /// Port of WhitelistCommand.
    fn cmd_whitelist(&mut self, lists: &Arc<Mutex<Lists>>, id: &str, ctx: &Context) -> R<i32> {
        match id {
            "server.whitelist.on" | "server.whitelist.off" => {
                let on = id == "server.whitelist.on";
                let was = std::mem::replace(&mut lists.lock().unwrap().use_whitelist, on);
                match (was, on) {
                    (true, true) => return Err(err("commands.whitelist.alreadyOn", &[])),
                    (false, false) => return Err(err("commands.whitelist.alreadyOff", &[])),
                    _ => {}
                }
                // DedicatedServer.setUsingWhitelist stores `white-list`.
                self.store_property("white-list", on.to_string());
                if on {
                    self.feedback(Text::translate("commands.whitelist.enabled", &[]));
                    self.flush_admin_feedback();
                    self.kick_unlisted_players(lists);
                } else {
                    self.feedback(Text::translate("commands.whitelist.disabled", &[]));
                }
                Ok(1)
            }
            "server.whitelist.list" => {
                let names: Vec<String> = lists
                    .lock()
                    .unwrap()
                    .white
                    .entries()
                    .iter()
                    .map(|e| e.user.name.clone())
                    .collect();
                if names.is_empty() {
                    self.feedback(Text::translate("commands.whitelist.none", &[]));
                } else {
                    self.feedback(Text::tr(
                        "commands.whitelist.list",
                        &[&names.len(), &names.join(", ")],
                    ));
                }
                Ok(names.len() as i32)
            }
            "server.whitelist.reload" => {
                // DedicatedPlayerList.reloadWhiteList.
                let _ = lists.lock().unwrap().white.load();
                self.feedback(Text::translate("commands.whitelist.reloaded", &[]));
                self.flush_admin_feedback();
                self.kick_unlisted_players(lists);
                Ok(1)
            }
            "server.whitelist.add" | "server.whitelist.remove" => {
                let add = id == "server.whitelist.add";
                let mut count = 0;
                for user in self.profiles(ctx)? {
                    let changed = {
                        let mut l = lists.lock().unwrap();
                        match (add, l.white.contains(&user)) {
                            (true, false) => l.white.add(WhiteEntry { user: user.clone() }),
                            (false, true) => l.white.remove(&user),
                            _ => false,
                        }
                    };
                    if changed {
                        count += 1;
                        let key = if add {
                            "commands.whitelist.add.success"
                        } else {
                            "commands.whitelist.remove.success"
                        };
                        self.feedback(Text::translate(key, &[Text::literal(user.name)]));
                    }
                }
                match (count, add) {
                    (0, true) => Err(err("commands.whitelist.add.failed", &[])),
                    (0, false) => Err(err("commands.whitelist.remove.failed", &[])),
                    _ => {
                        if !add {
                            self.flush_admin_feedback();
                            self.kick_unlisted_players(lists);
                        }
                        Ok(count)
                    }
                }
            }
            _ => Err(err("command.failed", &[])),
        }
    }
}
