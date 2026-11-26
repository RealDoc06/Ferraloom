//! Horizon mods in the game (`docs/mods.md`): loading the ones turned on,
//! their tick, frame, chat and command hooks, their HUD panels and what
//! they print. With no mod on, `horizon_mods::Mods` holds no engine and
//! every hook here returns after one check.
//!
//! Code mods run only in builds with the `mods` feature (release builds):
//! without it the game links no wasmtime, and every hook does nothing.
//! Content mods (`content_root`) load in every build.
use super::Game;
use horizon_client::mod_list;
use std::path::PathBuf;
#[cfg(feature = "mods")]
use {
    super::{execution_role::ExecutionRole, lan::LanHost, server_mods::ModEvent},
    horizon_client::{
        command::{Dispatcher, vanilla},
        mod_list::Installed,
    },
    horizon_mods::{Mods, Output, Runs, ServerPlayer, Snapshot, Status},
    std::time::Instant,
};

#[derive(Default)]
pub(super) struct ModsState {
    #[cfg(feature = "mods")]
    pub(super) mods: Mods,
    /// Capability `input`: a claimed item's use while the button is down.
    #[cfg(feature = "mods")]
    pub(super) using: Option<super::mod_input::ModUse>,
    /// `--dev-mod PATH`: always on, reloaded when rebuilt.
    pub(super) dev: Vec<PathBuf>,
    /// When dev mods were last checked for a rebuild.
    #[cfg(feature = "mods")]
    checked: Option<Instant>,
    /// The chat's command tree while a running mod owns commands.
    #[cfg(feature = "mods")]
    tree: Option<Dispatcher>,
    /// A server mod runs here: what players do is queued for it.
    #[cfg(feature = "mods")]
    pub(super) server: bool,
    /// The players server mods saw last tick, for joins and leaves.
    #[cfg(feature = "mods")]
    pub(super) players: Vec<ServerPlayer>,
    /// What players did this tick, for server mods at its end.
    #[cfg(feature = "mods")]
    pub(super) events: Vec<ModEvent>,
    /// A running server mod has capability `world`: its calls get the
    /// levels' chunks to read.
    #[cfg(feature = "mods")]
    pub(super) world: bool,
    /// Blocks server mods set, by dimension, made before that level's next
    /// tick.
    #[cfg(feature = "mods")]
    pub(super) block_writes: Vec<(String, horizon_mods::BlockChange)>,
    /// `command.run` lines for this window's own world, run on the next tick:
    /// run at once, their chat lines would reach the mods while their output
    /// is still being shown, and a mod answering its own lines would recurse.
    #[cfg(feature = "mods")]
    commands: Vec<String>,
    /// `command.run-quiet` batches for this window's own world, by mod id,
    /// run on the next tick as `commands` are.
    #[cfg(feature = "mods")]
    quiet: Vec<(String, Vec<String>)>,
    /// A `ui` mod's screen or settings page while it is up.
    #[cfg(feature = "mods")]
    screen: Option<ModScreen>,
    /// Capability `recipes`'s tables, made once a mod reads them.
    #[cfg(feature = "mods")]
    pub(super) recipes: Option<std::sync::Arc<horizon_mods::Recipes>>,
}

/// A `ui` mod's own screen, or its settings page from Options > Mods: the
/// widgets the mod pushed, laid out and drawn with the menus' own widgets.
#[cfg(feature = "mods")]
struct ModScreen {}
impl ModsState {
    /// Whether a mod runs: the chat keeps its lines for them.
    pub(super) fn running(&self) -> bool {
        #[cfg(feature = "mods")]
        return self.mods.is_running();
        #[cfg(not(feature = "mods"))]
        false
    }
}
/// How often dev mods are checked for a rebuild.
#[cfg(feature = "mods")]
const RELOAD_CHECK: std::time::Duration = std::time::Duration::from_secs(1);

/// A vanilla command name: mods may not take it.
#[cfg(feature = "mods")]
fn vanilla_command(name: &str) -> bool {
    let d = vanilla::game_dispatcher();
    d.nodes[0]
        .children
        .iter()
        .any(|&c| d.nodes[c].name() == name)
}

/// `root` with the content mods that are on layered over it
/// (docs/modpacks.md): their blocks, items, data and assets. Read once at
/// start, before the resources load; a mod whose content is refused is left
/// out here and its row on the Mods page says why.
pub(super) fn content_root(
    root: &std::path::Path,
    enabled: &std::collections::BTreeSet<String>,
    dev: &[std::path::PathBuf],
) -> std::path::PathBuf {
    use horizon_client::mod_content::{self, ContentMod};
    use horizon_mods::{Capability, Content, Package};
    let here = std::env::current_dir().unwrap_or_default();
    let mut paths = horizon_mods::find(&here.join(mod_list::DIR));
    paths.retain(|p| !dev.contains(p));
    paths.extend(dev.iter().cloned());
    let mut mods: Vec<ContentMod> = Vec::new();
    for path in &paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        let on = enabled.contains(&m.id) || dev.contains(path);
        if !on
            || !m.capabilities.contains(&Capability::Registries)
            || mods.iter().any(|c| c.id == m.id)
        {
            continue;
        }
        let content = Content::read(path, m).and_then(|content| {
            // The first of two mods owning one namespace stays on.
            let own = (m.id.as_str(), content.namespaces.as_slice());
            match mods
                .iter()
                .find_map(|c| horizon_mods::content::clash((&c.id, &c.content.namespaces), own))
            {
                Some(e) => Err(e),
                None => Ok(content),
            }
        });
        match content {
            Ok(content) => mods.push(ContentMod {
                id: m.id.clone(),
                content,
            }),
            Err(e) => log::warn!("Horizon: mod {} content not loaded: {e}", m.id),
        }
    }
    match mod_content::resource_root(root, &mods, &here.join(".horizon-cache")) {
        Ok(out) => {
            if out != root {
                log::info!(
                    "Horizon: content from {} mod(s) in {}",
                    mods.len(),
                    out.display()
                );
            }
            out
        }
        Err(e) => {
            log::warn!("Horizon: content mods not loaded: {e}");
            root.to_owned()
        }
    }
}

/// A build without the `mods` feature: no mod runs.
#[cfg(not(feature = "mods"))]
impl Game<'_> {
    /// Says why the installed and dev mods don't run.
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let found = horizon_mods::find(&dir).len() + self.mods.dev.len();
        if found > 0 {
            log::warn!(
                "Horizon: {found} mod(s) not started: this build has no mod runtime (build with --features mods)"
            );
        }
    }
    pub(super) fn tick_mods(&mut self) {}
    pub(super) fn mod_command(&mut self, _command: &str) -> bool {
        false
    }
    pub(super) fn mods_chat(&mut self, _line: &str) {}
}

#[cfg(feature = "mods")]
impl Game<'_> {
    /// Reads `horizon-mods/` and the dev mods, and starts the ones turned on.
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let dev = &self.mods.dev;
        let mut paths = horizon_mods::find(&dir);
        paths.retain(|p| !dev.contains(p));
        paths.extend(dev.iter().cloned());
        // A dev mod is on whatever the list says.
        let dev_ids: Vec<String> = dev
            .iter()
            .filter_map(|p| horizon_mods::Package::read(p).ok())
            .map(|p| p.manifest.id)
            .collect();
        let enabled = &self.settings.mods.enabled;
        let on = |id: &str| enabled.contains(id) || dev_ids.iter().any(|d| d == id);
        let mods = Mods::load_for(self.mod_runs(), &paths, on, vanilla_command);
        self.mods.mods = mods;
        let chosen = &self.settings.mods;
        self.mods.mods.set_bindings(|id| chosen.keys_of(id));
        // Entries were replaced wholesale: a screen's owner index is stale.
        self.mods.screen = None;
        // Content mods may have changed the items: remade on the next read.
        self.mods.recipes = None;
        self.list_mods();
        self.sync_mod_commands();
        self.show_mod_output();
    }
    /// Which mods this game runs: a server the server mods, the window
    /// client mods, and both when it simulates the world itself (in-frame).
    fn mod_runs(&self) -> Runs {
        match self.options.execution_role {
            ExecutionRole::IntegratedServer | ExecutionRole::DedicatedServer => Runs::Server,
        }
    }
    /// The Mods page's rows, from what loaded.
    fn list_mods(&mut self) {
        self.mods.server = self.mods.mods.runs_server();
        self.mods.world = self.mods.mods.wants_world();
        let dev = &self.mods.dev;
        self.settings.mods.installed = self
            .mods
            .mods
            .entries()
            .iter()
            .map(|e| {
                let file = e.path.file_name().unwrap_or_default().to_string_lossy();
                let problem = match &e.status {
                    Status::Failed(reason) => Some(format!("Stopped: {reason}")),
                    _ => None,
                };
                let Some(m) = &e.manifest else {
                    return Installed {
                        id: String::new(),
                        title: file.into_owned(),
                        description: String::new(),
                        problem,
                        dev: false,
                        has_settings: false,
                    };
                };
                // What it may do first: the row cuts long descriptions.
                let may: Vec<&str> = m.capabilities.iter().map(|c| c.describe()).collect();
                let may = if may.is_empty() {
                    "Asks for nothing.".to_owned()
                } else {
                    format!("May {}.", may.join(", "))
                };
                Installed {
                    id: m.id.clone(),
                    title: format!("{} {}", m.name, m.version),
                    description: format!("{may} {}", m.description).trim().to_owned(),
                    problem,
                    dev: dev.contains(&e.path),
                    has_settings: e.has_ui_settings(),
                }
            })
            .collect();
    }
    /// The chat's tree after the running mods changed.
    fn sync_mod_commands(&mut self) {
        // In-frame, the window runs server mods too: their names, free text
        // here (the server parses their arguments).
        let server = self.mods.mods.server_commands().map(|c| c.name.as_str());
        let mut names = self.mods.mods.commands().chain(server).peekable();
        self.mods.tree = names
            .peek()
            .is_some()
            .then(|| vanilla::game_dispatcher().with_mod_commands(names));
    }

    /// What mods read this tick (or frame, for mods with a `frame` hook): the
    /// server's players as they were at the last tick's end.
    fn mod_snapshot(&mut self) -> Snapshot {
        let players = if self.mods.server {
            self.mods.players.clone()
        } else {
            Vec::new()
        };
        self.mod_snapshot_with(players)
    }
    /// What mods read, with these players on the server. A server runs only
    /// server mods, whose world (`horizon-server-mod`) reads the players: the
    /// rest of the snapshot is the client worlds' and stays empty.
    pub(super) fn mod_snapshot_with(&mut self, players: Vec<ServerPlayer>) -> Snapshot {
        Snapshot {
            players,
            ..Default::default()
        }
    }
    /// A mod's `command.run-quiet` lines (vanilla's commands only, as
    /// `command.run`): to a server that runs them quietly as one payload
    /// (`horizon_protocol::quiet`), to any other as chat commands, or for this
    /// window's own world next tick.
    fn mod_quiet_commands(&mut self, mod_id: String, lines: Vec<String>) {
        let lines: Vec<String> = lines
            .into_iter()
            .filter(|l| vanilla_command(l.split(' ').next().unwrap_or_default()))
            .collect();
        self.mods.quiet.push((mod_id, lines));
    }
    /// A quiet batch in this window's own world, as a server runs one
    /// (game/lan_quiet.rs): each line as the player typing it, no chat, a
    /// chat message refused; one line in the log.
    fn run_quiet_batch(&mut self, mod_id: &str, lines: &[String]) {
        let permission = self.ui.registries.permission;
        let mut failed = 0;
        for line in lines {
            let ok = self.signable_arguments(permission, line).is_empty()
                && self.run_command_quietly(line);
            failed += usize::from(!ok);
        }
        log::info!(
            "[mod {mod_id}] ran {} quiet command(s): {failed} failed",
            lines.len()
        );
    }
    /// A game tick.
    pub(super) fn tick_mods(&mut self) {
        // Here, at the player's level: shown inside a console call (a control
        // socket command set the hook off), they would run as the console.
        for line in std::mem::take(&mut self.mods.commands) {
            self.run_command(&line);
        }
        for (mod_id, lines) in std::mem::take(&mut self.mods.quiet) {
            self.run_quiet_batch(&mod_id, &lines);
        }
        if !self.mods.mods.is_running() {
            return;
        }
        self.tick_mod_use();
        let snapshot = self.mod_snapshot();
        self.mods.mods.set_snapshot(snapshot);
        self.mods.mods.tick(self.ticks);
        self.show_mod_output();
    }
    /// Restarts the dev mods rebuilt since the last check, once a second.
    pub(super) fn reload_dev_mods(&mut self) {
        if self.mods.dev.is_empty()
            || self
                .mods
                .checked
                .is_some_and(|t| t.elapsed() < RELOAD_CHECK)
        {
            return;
        }
        self.mods.checked = Some(Instant::now());
        if self.mods.mods.reload_changed(vanilla_command) {
            let chosen = &self.settings.mods;
            self.mods.mods.set_bindings(|id| chosen.keys_of(id));
            self.list_mods();
            self.sync_mod_commands();
            log::info!("Horizon: mods reloaded");
        }
    }
    /// `/command` typed in chat: true when a mod owns it.
    pub(super) fn mod_command(&mut self, command: &str) -> bool {
        if !self.mods.mods.is_running() {
            return false;
        }
        // The command may follow a teleport in the same tick.
        let snapshot = self.mod_snapshot();
        self.mods.mods.set_snapshot(snapshot);
        if !self.mods.mods.command(command) {
            return false;
        }
        self.show_mod_output();
        true
    }
    /// A chat line the player received.
    pub(super) fn mods_chat(&mut self, line: &str) {
        self.mods.mods.chat(line);
        self.show_mod_output();
    }
    /// What mods showed: client mods' lines into the chat, server mods'
    /// to their players.
    pub(super) fn show_mod_output(&mut self) {
        self.route_mod_output(None);
    }
    /// [`Game::show_mod_output`], with the hub when it is out of `self.lan`.
    pub(super) fn route_mod_output(&mut self, mut host: Option<&mut LanHost>) {
        for output in self.mods.mods.drain() {
            match output {
                // Vanilla's commands only: a mod running a mod command could loop.
                Output::Command { line } => {
                    let name = line.split(' ').next().unwrap_or_default();
                    if vanilla_command(name) {
                        self.mods.commands.push(line);
                    }
                }
                Output::QuietCommands { mod_id, lines } => self.mod_quiet_commands(mod_id, lines),
                Output::Stopped { name, reason } => {
                    self.list_mods();
                    self.sync_mod_commands();
                    if self.options.dedicated.is_some() {
                        super::dedicated::log("WARN", &format!("Mod {name} stopped: {reason}"));
                    }
                }
                server @ (Output::Tell { .. }
                | Output::Broadcast { .. }
                | Output::Reply { .. }
                | Output::SetBlocks { .. }) => {
                    self.route_server_output(host.as_deref_mut(), server)
                }
                // A client mod's (its chat lines, screens, camera shake and
                // recipe book): a server runs none.
                Output::Chat { .. }
                | Output::Feedback { .. }
                | Output::OpenScreen { .. }
                | Output::CloseScreen { .. }
                | Output::Shake { .. }
                | Output::PlaceRecipe { .. }
                | Output::Give { .. } => {}
            }
        }
        // A mod may set or clear its settings page in any call, not only
        // while loading: its row's Settings button follows.
        let rows = self.settings.mods.installed.iter_mut();
        for (row, e) in rows.zip(self.mods.mods.entries()) {
            row.has_settings = e.has_ui_settings();
        }
    }
}
