//! Ferraloom: Bukkit plugins on the dedicated server (docs/ferraloom/plugins.md).
//!
//! `ferraloom_plugins` runs the JVM and the Bukkit API; this module is the
//! game's side: [`GameHost`], the host over `Game`, and the calls the event
//! sites make, one file per API area (`plugins/players.rs`, `blocks.rs`,
//! `worlds.rs`, `server.rs`) as in the crate. A site in upstream code is one line behind
//! `ferraloom_plugins::listens`, so without a listener (and always without a
//! `plugins/` jar) it costs one branch.
//!
//! The hub (`LanHost`) is where the players' connections are. Between ticks
//! it is in `Game::lan`; a guest's turn (`as_guest`) takes it out and lends
//! it to the events inside the turn ([`lend_hub`], `ferraloom_plugins::lend`);
//! the packet handlers hold it and pass it in. What
//! needs the hub in `Game::lan` (moving another player, a vanilla command)
//! and comes while it is not waits for the start of the next tick ([`Pending`]).
mod blocks;
// Entities.
pub(super) mod entities;
mod players;
mod server;
mod worlds;

use super::{Dim, Game, lan::LanHost};
use ferraloom_plugins::{Location, PlayerHost, PlayerId, WorldSpec};
use std::{
    cell::RefCell,
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

/// The worlds in Bukkit's order: the host's world indices.
const DIMS: [Dim; 3] = [Dim::Overworld, Dim::Nether, Dim::End];

fn world_index(dim: Dim) -> usize {
    DIMS.iter().position(|&d| d == dim).unwrap_or(0)
}

/// Work a host call could not do where it came, for the next tick's start.
enum Pending {
    Message(PlayerId, String),
    Teleport(PlayerId, Location),
    Vanilla(String),
    /// A guest's inventory, sent again (their client predicted a change a plugin cancelled).
    Inventory(PlayerId),
}

thread_local! {
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
    /// The labels plugin commands answer to: a command line reaches the JVM only when it starts with one.
    static LABELS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// `Server.shutdown()` was called.
static STOP: AtomicBool = AtomicBool::new(false);

fn pend(p: Pending) {
    PENDING.with_borrow_mut(|q| q.push(p));
}

/// Lends `hub` to the plugin calls `f` makes (a guest's turn holds the hub
/// outside `Game::lan`). Without plugins, one check.
pub(super) fn lend_hub<R>(hub: &mut LanHost, f: impl FnOnce() -> R) -> R {
    ferraloom_plugins::lend(hub, f)
}

/// The game as the Bukkit API sees it during one call into plugins.
struct GameHost<'a, 'g> {
    game: &'a mut Game<'g>,
    /// The hub when the caller holds it (a packet handler, a guest's turn);
    /// none when it is in `game.lan` (or there is none).
    hub: Option<&'a mut LanHost>,
}

impl<'a, 'g> GameHost<'a, 'g> {
    fn new(game: &'a mut Game<'g>, hub: Option<&'a mut LanHost>) -> Self {
        Self { game, hub }
    }

    /// The hub wherever it is now.
    fn hub(&mut self) -> Option<&mut LanHost> {
        match &mut self.hub {
            Some(h) => Some(h),
            None => self.game.lan.as_mut(),
        }
    }

    /// Whether the hub is in `Game::lan`, where commands and `for_player` find it.
    fn hub_in_game(&self) -> bool {
        self.hub.is_none() && self.game.lan.is_some()
    }
}

/// Plugin command labels arrived: the set commands are checked against, and
/// every player's command tree.
fn set_labels(game: &mut Game, labels: Vec<String>) {
    let tree = ferraloom_plugins::command_tree(&labels);
    LABELS.with_borrow_mut(|l| *l = labels.into_iter().map(|l| l.to_lowercase()).collect());
    if let Some(host) = game.lan.as_mut() {
        host.hub.set_plugin_commands(tree);
    }
}

/// Whether `line`'s first word is a plugin command's label.
fn is_plugin_command(line: &str) -> bool {
    let label = line.split(' ').next().unwrap_or("").to_lowercase();
    LABELS.with_borrow(|l| l.contains(&label))
}

impl Game<'_> {
    /// CraftServer's constructor and loadPlugins, before the world loads:
    /// a JVM and every plugin's onLoad, when `plugins/` holds a jar. A
    /// server without plugins loads nothing.
    pub(super) fn start_plugins(
        &mut self,
        level_name: &str,
        props: &horizon_server_files::properties::ServerProperties,
        port: u16,
    ) {
        let Ok(directory) = std::env::current_dir() else {
            return;
        };
        let names = [
            level_name.to_owned(),
            format!("{level_name}_nether"),
            format!("{level_name}_the_end"),
        ];
        let worlds: Vec<WorldSpec> = names
            .iter()
            .zip(0..)
            .map(|(name, environment)| WorldSpec { name, environment })
            .collect();
        let version = format!(
            "{}-Ferraloom (MC: {})",
            env!("CARGO_PKG_VERSION"),
            horizon_core::VANILLA_VERSION
        );
        let config = ferraloom_plugins::Config {
            directory: &directory,
            version: &version,
            worlds: &worlds,
            max_players: props.max_players,
            port: i32::from(port),
            motd: &props.motd,
            online_mode: props.online_mode,
            log: |level, thread, message| super::dedicated::log_from(thread, level, message),
        };
        if let Err(e) = ferraloom_plugins::start(&config, &mut GameHost::new(self, None)) {
            super::dedicated::log("ERROR", &format!("No plugins loaded: {e}"));
        }
        // Entities: the sites inside mob code.
        if ferraloom_plugins::running() {
            entities::install();
        }
    }

    /// Every plugin's onEnable, once the worlds are loaded (before "Done").
    pub(super) fn enable_plugins(&mut self) {
        if ferraloom_plugins::running() {
            ferraloom_plugins::enable(&mut GameHost::new(self, None));
            blocks::sync_level_hooks();
        }
    }

    /// Every plugin's onDisable, as the server stops.
    pub(super) fn disable_plugins(&mut self) {
        if ferraloom_plugins::running() {
            ferraloom_plugins::disable(&mut GameHost::new(self, None));
            blocks::sync_level_hooks();
        }
    }

    /// The start of a server tick (MinecraftServer.tickChildren's scheduler
    /// heartbeat): what waited for the hub, then the plugins' tick. True
    /// when a plugin asked the server to stop.
    pub(super) fn tick_plugins(&mut self) -> bool {
        if !ferraloom_plugins::running() {
            return false;
        }
        for p in PENDING.take() {
            match p {
                Pending::Message(id, text) => GameHost::new(self, None).send_message(id, &text),
                Pending::Teleport(id, to) => {
                    GameHost::new(self, None).teleport(id, to);
                }
                Pending::Vanilla(line) => {
                    server::run_vanilla(self, &line);
                }
                Pending::Inventory(id) => {
                    if let Some(host) = self.lan.as_mut()
                        && let Some(i) = host
                            .hub
                            .guests
                            .iter()
                            .position(|g| g.uuid == id && g.placed)
                    {
                        host.hub.send_inventory(i);
                    }
                }
            }
        }
        let tick = self.ticks as i64;
        if let Some(labels) = ferraloom_plugins::tick(&mut GameHost::new(self, None), tick) {
            set_labels(self, labels);
        }
        blocks::sync_level_hooks();
        STOP.swap(false, Ordering::Relaxed)
    }
}
