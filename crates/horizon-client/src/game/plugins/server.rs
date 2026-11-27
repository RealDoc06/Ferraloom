//! The server area: the console, vanilla commands, shutdown, and the
//! dedicated server's player lists.
use super::{GameHost, Pending, STOP, is_plugin_command, pend};
use crate::game::{
    Game,
    dedicated::{self, Lists},
    lan::LanHost,
};
use ferraloom_plugins::{
    Ban, BanTarget, List, PlayerId, Profile, ServerHost, ServerLists, WhitelistFlag,
};
use horizon_client::{
    server::{
        java_properties::now_millis,
        profiles,
        properties::{ServerProperties, SystemEntropy},
        user_cache::UserCache,
        user_lists::{BanEntry, NameAndId, OpEntry, WhiteEntry},
    },
    text::Text,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

impl ServerHost for GameHost<'_, '_> {
    fn console_message(&mut self, message: &str) {
        dedicated::log("INFO", &Text::legacy(message).plain());
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
        dedicated::log("INFO", &format!("System chat: {}", text.plain()));
    }
    ran
}

impl Game<'_> {
    /// ServerGamePacketListenerImpl.handleCustomCommandSuggestions for a
    /// plugin command (BukkitCommandWrapper.getSuggestions): the plugin's
    /// completions of the last word. False, at the cost of one check, when
    /// no plugin runs or the line is not a plugin command.
    pub(in crate::game) fn plugin_suggestions(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        id: i32,
        text: &str,
    ) -> bool {
        let skip = usize::from(text.starts_with('/'));
        if !ferraloom_plugins::running()
            || !text[skip..].contains(' ')
            || !is_plugin_command(&text[skip..])
        {
            return false;
        }
        let Some(player) = host.hub.guest(conn).map(|g| g.uuid) else {
            return false;
        };
        let completions = ferraloom_plugins::tab_complete(
            &mut GameHost::new(self, Some(&mut *host)),
            player,
            &text[skip..],
        );
        // SuggestionsBuilder.createOffset(input.lastIndexOf(' ') + 1), in UTF-16 units.
        let start = text.rfind(' ').map_or(0, |i| i + 1);
        let utf16 = |s: &str| s.encode_utf16().count() as i32;
        let mut w = horizon_protocol::buf::Writer::new();
        w.varint(id);
        w.varint(utf16(&text[..start]));
        w.varint(utf16(&text[start..]));
        w.varint(completions.len() as i32);
        for c in &completions {
            w.string(c);
            w.bool(false);
        }
        host.hub.send_to(
            conn,
            &horizon_protocol::packets::play::Clientbound::Unknown(
                horizon_protocol::ids::play::clientbound::COMMAND_SUGGESTIONS,
                w.data,
            ),
        );
        true
    }

    /// A console line: true when a plugin command took it.
    pub(in crate::game) fn plugin_console_command(&mut self, line: &str) -> bool {
        if !ferraloom_plugins::running() || !is_plugin_command(line) {
            return false;
        }
        ferraloom_plugins::console_command(&mut GameHost::new(self, None), line)
    }
}

/// Operators changed through a plugin: the hub's permission levels and
/// command trees follow at the next tick's start.
static OPS_CHANGED: AtomicBool = AtomicBool::new(false);

/// The dedicated server's lists and user cache as plugins see them
/// (`Server.getOfflinePlayer`, `getBanList`, `OfflinePlayer.setOp`...),
/// answered on any thread under the lists' own locks. Changes follow
/// CraftBukkit's: saved at once, no kicks, an operator's new level sent at
/// the next tick. `lists` is the dedicated server's `Admin::lists` (the same
/// `Arc`, given before `Admin` exists, since plugins load first).
struct GameLists {
    lists: Arc<Mutex<Lists>>,
    cache: &'static Mutex<Option<UserCache>>,
    /// `op-permission-level`, until `Admin` holds it.
    op_level: i32,
}

fn name_and_id(p: &Profile) -> NameAndId {
    NameAndId {
        id: p.id,
        name: p.name.clone(),
    }
}

fn profile(user: &NameAndId) -> Profile {
    Profile {
        id: user.id,
        name: user.name.clone(),
    }
}

impl ServerLists for GameLists {
    fn profile_by_name(&self, name: &str) -> Option<Profile> {
        let mut cache = self.cache.lock().unwrap();
        let found = cache.as_mut()?.get_by_name(name, |lower| {
            let found = profiles::find_by_name(&profiles::discovery_url(), lower);
            if found.is_none() {
                dedicated::log("WARN", &format!("Couldn't find profile with name: {lower}"));
            }
            found
        });
        found.as_ref().map(profile)
    }

    fn profile_by_id(&self, id: PlayerId) -> Option<Profile> {
        self.cache
            .lock()
            .unwrap()
            .as_mut()?
            .get_by_uuid(id)
            .as_ref()
            .map(profile)
    }

    fn listed(&self, list: List) -> Vec<Profile> {
        let l = self.lists.lock().unwrap();
        match list {
            List::Ops => l.ops.entries().iter().map(|e| profile(&e.user)).collect(),
            List::Whitelist => l.white.entries().iter().map(|e| profile(&e.user)).collect(),
            List::Banned => l.bans.entries().iter().map(|e| profile(&e.user)).collect(),
        }
    }

    fn is_listed(&self, list: List, id: PlayerId) -> bool {
        let user = NameAndId {
            id,
            name: String::new(),
        };
        let mut l = self.lists.lock().unwrap();
        match list {
            List::Ops => l.ops.contains(&user),
            List::Whitelist => l.white.contains(&user),
            // UserBanList.isBanned: an expired ban is dropped first.
            List::Banned => l.bans.get(&user).is_some(),
        }
    }

    /// PlayerList.op and deop, and the whitelist's add and remove.
    fn set_listed(&self, list: List, who: &Profile, on: bool) {
        let user = name_and_id(who);
        // Read before the lists' lock: never hold it while taking `Admin`'s.
        let level = dedicated::admin(|a| a.op_level).unwrap_or(self.op_level);
        let mut l = self.lists.lock().unwrap();
        match (list, on) {
            (List::Ops, true) => {
                let bypasses_player_limit = l
                    .ops
                    .entries()
                    .iter()
                    .any(|e| e.user.id == who.id && e.bypasses_player_limit);
                l.ops.add(OpEntry {
                    user,
                    level,
                    bypasses_player_limit,
                });
                OPS_CHANGED.store(true, Ordering::Relaxed);
            }
            (List::Ops, false) => {
                l.ops.remove(&user);
                OPS_CHANGED.store(true, Ordering::Relaxed);
            }
            (List::Whitelist, true) => {
                l.white.add(WhiteEntry { user });
            }
            (List::Whitelist, false) => {
                l.white.remove(&user);
            }
            (List::Banned, _) => {}
        }
    }

    fn whitelist_flag(&self, flag: WhitelistFlag) -> bool {
        let l = self.lists.lock().unwrap();
        match flag {
            WhitelistFlag::On => l.use_whitelist,
            WhitelistFlag::Enforced => l.enforce_whitelist,
        }
    }

    /// DedicatedServer.setUsingWhitelist and setEnforceWhitelist: the flag and server.properties.
    fn set_whitelist_flag(&self, flag: WhitelistFlag, on: bool) {
        let key = {
            let mut l = self.lists.lock().unwrap();
            match flag {
                WhitelistFlag::On => {
                    l.use_whitelist = on;
                    "white-list"
                }
                WhitelistFlag::Enforced => {
                    l.enforce_whitelist = on;
                    "enforce-whitelist"
                }
            }
        };
        let path = std::path::Path::new("server.properties");
        if let Ok(p) = ServerProperties::from_file(path, &mut SystemEntropy::default())
            && let Ok(p) = p.update(key, on.to_string())
        {
            let _ = p.store(path);
        }
    }

    fn reload_whitelist(&self) {
        let _ = self.lists.lock().unwrap().white.load();
    }

    fn bans(&self, ip: bool) -> Vec<Ban> {
        let now = now_millis();
        let l = self.lists.lock().unwrap();
        let live = |expires: Option<i64>| expires.is_none_or(|e| e >= now);
        if ip {
            l.ip_bans
                .entries()
                .iter()
                .filter(|e| live(e.expires))
                .map(|e| Ban {
                    target: BanTarget::Ip(e.user.clone()),
                    created: e.created,
                    source: e.source.clone(),
                    expires: e.expires,
                    reason: e.reason.clone(),
                })
                .collect()
        } else {
            l.bans
                .entries()
                .iter()
                .filter(|e| live(e.expires))
                .map(|e| Ban {
                    target: BanTarget::Player(profile(&e.user)),
                    created: e.created,
                    source: e.source.clone(),
                    expires: e.expires,
                    reason: e.reason.clone(),
                })
                .collect()
        }
    }

    fn add_ban(
        &self,
        target: BanTarget,
        created: Option<i64>,
        source: Option<String>,
        expires: Option<i64>,
        reason: Option<String>,
    ) {
        let mut l = self.lists.lock().unwrap();
        match target {
            BanTarget::Player(p) => {
                let mut ban = BanEntry::new(name_and_id(&p), source, expires, reason);
                ban.created = created.unwrap_or(ban.created);
                l.bans.add(ban);
            }
            BanTarget::Ip(ip) => {
                let mut ban = BanEntry::new(ip, source, expires, reason);
                ban.created = created.unwrap_or(ban.created);
                l.ip_bans.add(ban);
            }
        }
    }

    fn pardon(&self, target: &BanTarget) {
        let mut l = self.lists.lock().unwrap();
        match target {
            BanTarget::Player(p) => {
                l.bans.remove(&name_and_id(p));
            }
            BanTarget::Ip(ip) => {
                l.ip_bans.remove(ip);
            }
        }
    }
}

/// The dedicated server's lists (`Admin::lists`), before the plugins start:
/// one allocation, plugins or not.
pub(in crate::game) fn register_lists(
    lists: &Arc<Mutex<Lists>>,
    cache: &'static Mutex<Option<UserCache>>,
    op_level: i32,
) {
    ferraloom_plugins::set_lists(Box::new(GameLists {
        lists: lists.clone(),
        cache,
        op_level,
    }));
}

/// At a tick's start (plugins running): operators a plugin changed reach
/// the hub, as `/op`'s do, and players who joined since, or whose plugin
/// called `Player.updateCommands`, get their command tree again.
pub(super) fn apply_list_changes(game: &mut Game) {
    if OPS_CHANGED.swap(false, Ordering::Relaxed)
        && let Some(lists) = dedicated::admin(|a| a.lists.clone())
    {
        game.sync_ops(&lists);
    }
    let updates = ferraloom_plugins::take_command_updates();
    if !updates.is_empty()
        && let Some(host) = game.lan.as_mut()
    {
        for player in updates {
            host.hub.resend_plugin_commands(player);
        }
    }
}
