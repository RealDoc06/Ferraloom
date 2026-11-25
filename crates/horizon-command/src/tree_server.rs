//! The commands vanilla registers only on a dedicated server
//! (`Commands.CommandSelection.DEDICATED`), in its registration order. Their
//! ids start with `server.`; the game runs them in
//! `game/commands/server_cmds.rs`. `/perf` is left out: Horizon has no
//! metrics recorder for it to start.
use super::{
    Dispatcher,
    args::{ArgType, integer},
    vanilla::PLAYERS,
};

const ADMINS: u8 = 3;
const OWNERS: u8 = 4;

/// Port of the `register` methods of BanIpCommands, BanListCommands,
/// BanPlayerCommands, DeOpCommands, OpCommand, PardonCommand,
/// PardonIpCommand, SaveAllCommand, SaveOffCommand, SaveOnCommand,
/// SetPlayerIdleTimeoutCommand, StopCommand, TransferCommand and
/// WhitelistCommand.
pub fn register(d: &mut Dispatcher, root: usize) {
    // ban-ip <target> [<reason>]
    let ban_ip = d.literal(root, "ban-ip");
    d.requires(ban_ip, ADMINS);
    let target = d.argument(ban_ip, "target", ArgType::Word);
    d.executes(target, "server.ban-ip");
    let reason = d.argument(target, "reason", ArgType::Message);
    d.executes(reason, "server.ban-ip");

    // banlist [ips|players]
    let banlist = d.literal(root, "banlist");
    d.requires(banlist, ADMINS);
    d.executes(banlist, "server.banlist");
    for which in ["ips", "players"] {
        let l = d.literal(banlist, which);
        d.executes(l, "server.banlist");
    }

    // ban <targets> [<reason>]
    let ban = d.literal(root, "ban");
    d.requires(ban, ADMINS);
    let targets = d.argument(ban, "targets", ArgType::GameProfile);
    d.executes(targets, "server.ban");
    let reason = d.argument(targets, "reason", ArgType::Message);
    d.executes(reason, "server.ban");

    // deop, op, pardon <targets>
    for (name, id) in [
        ("deop", "server.deop"),
        ("op", "server.op"),
        ("pardon", "server.pardon"),
    ] {
        let l = d.literal(root, name);
        d.requires(l, ADMINS);
        let targets = d.argument(l, "targets", ArgType::GameProfile);
        d.executes(targets, id);
    }

    // pardon-ip <target>
    let pardon_ip = d.literal(root, "pardon-ip");
    d.requires(pardon_ip, ADMINS);
    let target = d.argument(pardon_ip, "target", ArgType::Word);
    d.executes(target, "server.pardon-ip");

    // save-all [flush], save-off, save-on
    let save_all = d.literal(root, "save-all");
    d.requires(save_all, OWNERS);
    d.executes(save_all, "server.save-all");
    let flush = d.literal(save_all, "flush");
    d.executes(flush, "server.save-all");
    for (name, id) in [
        ("save-off", "server.save-off"),
        ("save-on", "server.save-on"),
    ] {
        let l = d.literal(root, name);
        d.requires(l, OWNERS);
        d.executes(l, id);
    }

    // setidletimeout <minutes>
    let idle = d.literal(root, "setidletimeout");
    d.requires(idle, ADMINS);
    let minutes = d.argument(idle, "minutes", integer(0));
    d.executes(minutes, "server.setidletimeout");

    // stop
    let stop = d.literal(root, "stop");
    d.requires(stop, OWNERS);
    d.executes(stop, "server.stop");

    // transfer <hostname> [<port> [<players>]]
    let transfer = d.literal(root, "transfer");
    d.requires(transfer, ADMINS);
    let host = d.argument(transfer, "hostname", ArgType::Str);
    d.executes(host, "server.transfer");
    let port = d.argument(host, "port", ArgType::Integer { min: 1, max: 65535 });
    d.executes(port, "server.transfer");
    let players = d.argument(port, "players", PLAYERS);
    d.executes(players, "server.transfer");

    // whitelist on|off|list|add <targets>|remove <targets>|reload
    let whitelist = d.literal(root, "whitelist");
    d.requires(whitelist, ADMINS);
    for (name, id) in [
        ("on", "server.whitelist.on"),
        ("off", "server.whitelist.off"),
        ("list", "server.whitelist.list"),
    ] {
        let l = d.literal(whitelist, name);
        d.executes(l, id);
    }
    for (name, id) in [
        ("add", "server.whitelist.add"),
        ("remove", "server.whitelist.remove"),
    ] {
        let l = d.literal(whitelist, name);
        let targets = d.argument(l, "targets", ArgType::GameProfile);
        d.executes(targets, id);
    }
    let reload = d.literal(whitelist, "reload");
    d.executes(reload, "server.whitelist.reload");
}

/// Commands.<init> with `CommandSelection.DEDICATED`: the integrated tree
/// without `/publish` and `/unpublish`, `/seed` and `/version` for
/// gamemasters only (their `checkPermissions`), and the dedicated-only
/// commands after the others.
pub(crate) fn make_dedicated(d: &mut Dispatcher) {
    for name in ["seed", "version"] {
        if let Some(n) = d.find(name) {
            d.requires(n, super::vanilla::GAMEMASTERS);
        }
    }
    let integrated: Vec<usize> = ["publish", "unpublish"]
        .into_iter()
        .filter_map(|name| d.find(name))
        .collect();
    d.nodes[0].children.retain(|c| !integrated.contains(c));
    register(d, 0);
}
