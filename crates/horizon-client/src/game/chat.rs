//! Chat, the command registries and the per-world state behind commands
//! (game rules, the world clock, difficulty), bound to the game.
use super::Game;
use horizon_client::{
    chat::{ChatAbilities, ChatSource, ChatVisibility},
    command::Registries,
    gamerules::{GameRules, RuleId, rule_id},
    text::Text,
};
use std::collections::BTreeMap;

/// The local player's name (offline single player).
pub const PLAYER_NAME: &str = "Player";

/// ServerClockManager.ServerClockInstance for the overworld clock (its
/// total ticks are the world's day time).
#[derive(Clone, Copy, Debug)]
pub struct WorldClock {
    pub partial: f32,
    pub rate: f32,
    pub paused: bool,
}
impl Default for WorldClock {
    fn default() -> Self {
        Self {
            partial: 0.0,
            rate: 1.0,
            paused: false,
        }
    }
}
/// A clock time marker from the timelines (ClockTimeMarker).
#[derive(Clone, Copy, Debug)]
pub struct TimeMarker {
    pub ticks: i64,
    pub period: Option<i64>,
    pub show: bool,
}
impl TimeMarker {
    pub fn occurs_at(&self, total: i64) -> bool {
        match self.period {
            Some(p) => self.ticks == total.rem_euclid(p),
            None => self.ticks == total,
        }
    }
    pub fn resolve(&self, total: i64) -> i64 {
        match self.period {
            Some(p) => {
                let d = self.ticks - total.rem_euclid(p);
                total + if d > 0 { d } else { p + d }
            }
            None => self.ticks,
        }
    }
}
/// UI and command state that lives beside the game.
#[derive(Default)]
pub struct UiState {
    pub registries: Registries,
    pub rules: GameRules,
    pub clock: WorldClock,
    /// Time markers and timelines of the overworld clock.
    pub markers: BTreeMap<String, TimeMarker>,
    pub timelines: BTreeMap<String, Option<i64>>,
    /// 0 peaceful .. 3 hard.
    pub difficulty: u8,
    /// Death message key set by the cause of death (e.g. /kill).
    pub death_cause: Option<String>,
    /// The prepared resources before resource packs are layered on.
    pub vanilla_root: std::path::PathBuf,
    /// Death screen cause and when the player died (buttons wait 20 ticks).
    pub death_text: Option<Text>,
    pub died_at: Option<std::time::Instant>,
    /// `DeathScreen.TitleConfirmScreen` and when it opened (its buttons wait
    /// 20 ticks, `setDelay`).
    pub death_confirm: Option<std::time::Instant>,
    /// `LivingEntity.deathTime` of the window's player: ticks since it died.
    pub death_time: u32,
}

impl Game<'_> {
    /// `PlayerList.isOp` for the single-player owner: the world's Allow Commands.
    pub(super) fn allow_commands(&self) -> bool {
        self.level.allow_commands == Some(true)
    }
    /// Commands the game issues itself (the server console, harnesses, the
    /// control socket) run at the owner's level whatever Allow Commands says.
    pub(super) fn as_console<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let level = std::mem::replace(&mut self.ui.registries.permission, 4);
        let out = f(self);
        self.ui.registries.permission = level;
        out
    }
    /// Registry contents for argument parsing (built when a world opens).
    pub(super) fn build_registries(&mut self) {
        let permission = owner_permission(self.allow_commands());
        let reg = &mut self.ui.registries;
        reg.permission = permission;
        {
            let dir = |sub: &str| -> Vec<String> {
                let root = self.options.resources.join("data/minecraft").join(sub);
                let mut out = Vec::new();
                let mut stack = vec![root.clone()];
                while let Some(d) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&d) else {
                        continue;
                    };
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().and_then(|x| x.to_str()) == Some("json")
                            && let Ok(rel) = p.strip_prefix(&root)
                        {
                            out.push(format!(
                                "minecraft:{}",
                                rel.with_extension("").to_string_lossy().replace('\\', "/")
                            ));
                        }
                    }
                }
                out.sort();
                out
            };
            self.ui.registries.recipes = dir("recipe");
            self.ui.registries.damage_types = dir("damage_type");
            self.ui.registries.loot_tables = dir("loot_table");
            self.ui.registries.templates = {
                let root = self.options.resources.join("worldgen/structure/minecraft");
                let mut out = Vec::new();
                let mut stack = vec![root.clone()];
                while let Some(d) = stack.pop() {
                    let Ok(entries) = std::fs::read_dir(&d) else {
                        continue;
                    };
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().and_then(|x| x.to_str()) == Some("nbt")
                            && let Ok(rel) = p.strip_prefix(&root)
                        {
                            out.push(format!(
                                "minecraft:{}",
                                rel.with_extension("").to_string_lossy().replace('\\', "/")
                            ));
                        }
                    }
                }
                out.sort();
                out
            };
        }
        self.ui.registries.sounds = self.audio.event_names();
        self.ui.registries.attributes = [
            "armor",
            "armor_toughness",
            "attack_damage",
            "attack_knockback",
            "attack_speed",
            "block_break_speed",
            "block_interaction_range",
            "burning_time",
            "camera_distance",
            "explosion_knockback_resistance",
            "entity_interaction_range",
            "fall_damage_multiplier",
            "flying_speed",
            "follow_range",
            "gravity",
            "jump_strength",
            "knockback_resistance",
            "luck",
            "max_absorption",
            "max_health",
            "mining_efficiency",
            "movement_efficiency",
            "movement_speed",
            "oxygen_bonus",
            "safe_fall_distance",
            "scale",
            "sneaking_speed",
            "spawn_reinforcements",
            "step_height",
            "submerged_mining_speed",
            "sweeping_damage_ratio",
            "tempt_range",
            "water_movement_efficiency",
        ]
        .iter()
        .map(|a| format!("minecraft:{a}"))
        .collect();
        let reg = &mut self.ui.registries;
        let mut items: Vec<String> = self
            .res
            .items()
            .order
            .iter()
            .map(|s| s.to_string())
            .collect();
        items.sort();
        reg.items = items;
        reg.item_tags = self
            .res
            .items()
            .item_tags
            .keys()
            .map(|k| horizon_client::command::args::normalize(k))
            .collect();
        reg.item_tags.sort();
        reg.block_tags = self
            .res
            .items()
            .block_tags
            .keys()
            .map(|k| horizon_client::command::args::normalize(k))
            .collect();
        reg.block_tags.sort();
        let registry = self.pack.assets.registry;
        let mut blocks: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
        for id in 0..registry.len() {
            let Some(state) = registry.state(horizon_world::registry::BlockStateId(id as u32))
            else {
                continue;
            };
            let props = blocks.entry(state.name.clone()).or_default();
            for (k, v) in &state.properties {
                let values = props.entry(k.clone()).or_default();
                if !values.contains(v) {
                    values.push(v.clone());
                }
            }
        }
        reg.blocks = blocks
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        let mut summonable: Vec<String> = self
            .res
            .mob_types()
            .keys()
            .map(|k| horizon_client::command::args::normalize(k))
            .collect();
        summonable.extend(
            [
                "minecraft:item",
                "minecraft:experience_orb",
                "minecraft:falling_block",
                "minecraft:arrow",
                "minecraft:lightning_bolt",
                "minecraft:minecart",
                "minecraft:chest_minecart",
                "minecraft:hopper_minecart",
                "minecraft:furnace_minecart",
                "minecraft:tnt_minecart",
            ]
            .map(String::from),
        );
        summonable.sort();
        summonable.dedup();
        reg.summonable = summonable;
        let root = self.options.resources.clone();
        let mut types: Vec<String> = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(root.join("entity-types.json")).unwrap_or_default(),
        )
        .ok()
        .and_then(|v| {
            v.as_object().map(|o| {
                o.keys()
                    .map(|k| horizon_client::command::args::normalize(k))
                    .collect()
            })
        })
        .unwrap_or_default();
        if types.is_empty() {
            types = reg.summonable.clone();
            types.push("minecraft:player".into());
        }
        types.sort();
        reg.entity_types = types;
        let list = |dir: &str| -> Vec<String> {
            let mut out: Vec<String> = std::fs::read_dir(root.join(dir))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_str()?
                        .strip_suffix(".json")
                        .map(|n| format!("minecraft:{n}"))
                })
                .collect();
            out.sort();
            out
        };
        // The mob effect registry: every effect `/effect` and the
        // `mob_effect` resource arguments accept.
        reg.effects = horizon_client::brewing::EFFECTS
            .iter()
            .map(|e| format!("minecraft:{}", e.id))
            .collect();
        reg.effects.sort();
        reg.enchantments = list("data/minecraft/enchantment");
        reg.structures = list("data/minecraft/worldgen/structure");
        reg.biomes = list("data/minecraft/worldgen/biome");
        reg.clocks = list("data/minecraft/world_clock");
        reg.dimensions = vec![
            "minecraft:overworld".into(),
            "minecraft:the_nether".into(),
            "minecraft:the_end".into(),
        ];
        reg.players = vec![PLAYER_NAME.into()];
        // Timelines of the overworld clock and their time markers.
        self.ui.markers.clear();
        self.ui.timelines.clear();
        for name in list("data/minecraft/timeline") {
            let path = root.join(format!(
                "data/minecraft/timeline/{}.json",
                name.trim_start_matches("minecraft:")
            ));
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(path).unwrap_or_default(),
            ) else {
                continue;
            };
            if v["clock"].as_str() != Some("minecraft:overworld") {
                continue;
            }
            let period = v["period_ticks"].as_i64();
            self.ui.timelines.insert(name.clone(), period);
            for (marker, def) in v["time_markers"].as_object().into_iter().flatten() {
                let (ticks, show) = match def {
                    serde_json::Value::Number(n) => (n.as_i64().unwrap_or(0), false),
                    other => (
                        other["ticks"].as_i64().unwrap_or(0),
                        other["show_in_commands"].as_bool().unwrap_or(false),
                    ),
                };
                self.ui.markers.insert(
                    marker.clone(),
                    TimeMarker {
                        ticks,
                        period,
                        show,
                    },
                );
            }
        }
        // Vanilla's built-in registries (mob effects, components, dialogs...).
        for dir in [&self.options.resources, &self.ui.vanilla_root] {
            if let Ok(bytes) = std::fs::read(dir.join("command-registries.json"))
                && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
            {
                self.ui.registries.fill_from_export(&v);
                break;
            }
        }
        let reg = &mut self.ui.registries;
        reg.timelines = self.ui.timelines.keys().cloned().collect();
        reg.time_markers = self
            .ui
            .markers
            .iter()
            .filter(|(_, m)| m.show)
            .map(|(k, _)| k.clone())
            .collect();
    }
    /// `Minecraft.computeChatAbilities` from the Chat option.
    pub(super) fn chat_abilities(&self) -> ChatAbilities {
        ChatAbilities::new(ChatVisibility::from_option(self.settings.chat_visibility))
    }
    /// A chat message from the game (ChatComponent.addServerSystemMessage).
    pub(super) fn system_message(&mut self, text: Text) {
        if let Some(capture) = self.chat_capture.as_mut() {
            capture.push(text);
            return;
        }
        // Connected to a server (single player's own included), the chat on
        // screen is the connection's: the game's own messages join it.
        // The Chat option's Hidden level takes the server's messages too.
        self.local_chat_line(text, ChatSource::Server);
    }
    /// A player's chat line on the window's own chat: a LAN guest's message
    /// while the host runs in-frame (`HostEvent::Chat`). `ChatListener`
    /// checks `canReceivePlayerMessages` for these, so Commands Only and
    /// Hidden drop them; command feedback (`/list`, `/tp`) is a system
    /// message and takes the system-message permission instead.
    pub(super) fn player_message(&mut self, text: Text) {
        if let Some(capture) = self.chat_capture.as_mut() {
            capture.push(text);
            return;
        }
        // A server's client receives the line as a packet (PlayerChat or
        // DisguisedChat) and filters it there.
        self.local_chat_line(text, ChatSource::Player);
    }
    /// A line for the window's own chat when the Chat option lets `source`
    /// through: a server shows none, but running mods hear it.
    fn local_chat_line(&mut self, text: Text, source: ChatSource) {
        if !self.chat_abilities().shows(source) {
            return;
        }
        if self.mods.running() {
            self.mods_chat(&text.plain());
        }
    }
    /// Advance the overworld clock (ServerClockManager.tick) when
    /// `advance_time` allows it.
    pub(super) fn advance_clock(&mut self) {
        if !self.ui.rules.bool("advance_time") || self.ui.clock.paused {
            return;
        }
        let c = &mut self.ui.clock;
        c.partial += c.rate;
        let whole = c.partial.floor();
        c.partial -= whole;
        self.world.day_time += whole as i64;
    }
    /// `ServerClockInstance.packNetworkState`: the rate players' clients
    /// advance the overworld clock at, 0 while it is paused (`/time pause`)
    /// or `advance_time` is off.
    pub(super) fn network_clock_rate(&self) -> f32 {
        if self.ui.rules.bool("advance_time") && !self.ui.clock.paused {
            self.ui.clock.rate
        } else {
            0.0
        }
    }
    /// The engine-side rule values (block behaviour and health).
    pub(super) fn level_rules(&self) -> horizon_world::level::GameRules {
        // Keep identities, not values: commands can change these rules between
        // scheduled ticks. Resolve names only on the first adapter construction.
        static IDS: std::sync::LazyLock<[RuleId; 9]> = std::sync::LazyLock::new(|| {
            [
                "water_source_conversion",
                "lava_source_conversion",
                "random_tick_speed",
                "block_drops",
                "mob_griefing",
                "max_snow_accumulation_height",
                "spawn_mobs",
                "spawn_monsters",
                "fire_spread_radius_around_player",
            ]
            .map(|name| rule_id(name).expect("registered level game rule"))
        });
        let [
            water,
            lava,
            random_ticks,
            drops,
            griefing,
            snow,
            mobs,
            monsters,
            fire_radius,
        ] = *IDS;
        let r = &self.ui.rules;
        horizon_world::level::GameRules {
            water_source_conversion: r.bool_by_id(water),
            lava_source_conversion: r.bool_by_id(lava),
            random_tick_speed: r.int_by_id(random_ticks),
            block_drops: r.bool_by_id(drops),
            mob_griefing: r.bool_by_id(griefing),
            max_snow_accumulation_height: r.int_by_id(snow),
            spawn_mobs: r.bool_by_id(mobs),
            spawn_monsters: r.bool_by_id(monsters),
            fire_spread_radius_around_player: r.int_by_id(fire_radius),
        }
    }
    /// Apply rules mirrored in other state (natural regeneration, difficulty).
    pub(super) fn sync_rules(&mut self) {
        self.world.health_rules.natural_regeneration =
            self.ui.rules.bool("natural_health_regeneration");
        // Player.isInvulnerableTo: the damage-type rules.
        self.world.health_rules.drowning_damage = self.ui.rules.bool("drowning_damage");
        self.world.health_rules.fall_damage = self.ui.rules.bool("fall_damage");
        self.world.health_rules.fire_damage = self.ui.rules.bool("fire_damage");
        self.world.health_rules.freeze_damage = self.ui.rules.bool("freeze_damage");
        use horizon_world::food::Difficulty;
        self.world.health_rules.difficulty = match self.ui.difficulty {
            0 => Difficulty::Peaceful,
            1 => Difficulty::Easy,
            3 => Difficulty::Hard,
            _ => Difficulty::Normal,
        };
    }
    /// Load per-world command state when a world opens.
    pub(super) fn load_world_state(&mut self) {
        self.ui.rules = GameRules::load(&self.world_root);
        self.ui.clock = WorldClock::default();
        // PrimaryLevelData: the difficulty comes back from level.dat's difficulty_settings.
        self.ui.difficulty = horizon_client::command::vanilla::DIFFICULTIES
            .iter()
            .position(|d| *d == self.level.difficulty)
            .map_or(2, |i| i as u8);
        if let Ok(bytes) = std::fs::read(self.world_root.join("data/minecraft/world_clocks.dat"))
            && let Ok(raw) = horizon_world::compression::Compression::Gzip.decode(&bytes, 1 << 20)
            && let Ok((root, _)) =
                horizon_world::nbt::read_named(&raw, horizon_world::nbt::Limits::default())
            && let Some(c) = root
                .tag
                .get("data")
                .and_then(|d| d.get("minecraft:overworld"))
        {
            use horizon_world::nbt::Tag;
            if let Some(Tag::Float(r)) = c.get("rate") {
                self.ui.clock.rate = f32::from_bits(*r);
            }
            if let Some(Tag::Float(p)) = c.get("partial_tick") {
                self.ui.clock.partial = f32::from_bits(*p);
            }
            if let Some(Tag::Byte(p)) = c.get("paused") {
                self.ui.clock.paused = *p != 0;
            }
        }
        self.build_registries();
        self.sync_rules();
    }
    /// Save the clock beside level.dat (Level.write saves the game rules: a
    /// second writer of game_rules.dat could leave it with the rules loaded
    /// when the world opened).
    pub(super) fn save_world_state(&mut self) {
        if self.world_root.as_os_str().is_empty() {
            return;
        }
        use horizon_world::nbt::Tag;
        let c = self.ui.clock;
        // The other clocks stay as saved; the overworld one keeps the fields Horizon
        // does not model, and (like vanilla's codec) omits rate, partial tick and
        // pause while they have their defaults.
        let path = self.world_root.join("data/minecraft/world_clocks.dat");
        let existing = horizon_client::save::read_dat(&path);
        let mut data = match existing.as_ref().and_then(|r| r.get("data")) {
            Some(d @ Tag::Compound(_)) => d.clone(),
            _ => Tag::Compound(Default::default()),
        };
        let mut clock = match data.get("minecraft:overworld") {
            Some(c @ Tag::Compound(_)) => c.clone(),
            _ => Tag::Compound(Default::default()),
        };
        if let Tag::Compound(map) = &mut clock {
            map.insert("total_ticks".into(), Tag::Long(self.world.day_time));
            for (key, value, default) in [
                (
                    "partial_tick",
                    Tag::Float(c.partial.to_bits()),
                    c.partial == 0.0,
                ),
                ("rate", Tag::Float(c.rate.to_bits()), c.rate == 1.0),
                ("paused", Tag::Byte(i8::from(c.paused)), !c.paused),
            ] {
                if default {
                    map.remove(&key.into());
                } else {
                    map.insert(key.into(), value);
                }
            }
        }
        if let Tag::Compound(map) = &mut data {
            map.insert("minecraft:overworld".into(), clock);
        }
        let root = Tag::compound([
            ("data", data),
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
        ]);
        if let Err(e) = horizon_client::save::write_dat(&path, root) {
            log::warn!("the world clocks were not written: {e}");
        }
    }
    /// Death message in chat (CombatTracker.getDeathMessage, as far as the
    /// cause is known to the client).
    pub(super) fn death_message(&mut self) {
        let cause = self.ui.death_cause.take();
        self.ui.died_at = Some(std::time::Instant::now());
        self.ui.death_text = None;
        self.ui.death_confirm = None;
        if !self.ui.rules.bool("show_death_messages") {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let below_world = p.position.y < f64::from(horizon_client::chunk::MIN_Y - 64);
        let name = self.player_display_text();
        let message = match cause {
            Some(key) => Text::translate(&key, &[name]),
            None if below_world => Text::translate("death.attack.outOfWorld", &[name]),
            None => self.combat_death_message(),
        };
        self.ui.death_text = Some(message.clone());
        // ServerPlayer.die: a team that hides death messages keeps it out of
        // the dying player's own chat (the death screen still shows it).
        if horizon_client::scoreboard::own_death_message_shown(self.player_team()) {
            self.system_message(message);
        }
    }
}

/// `IntegratedServer.getProfilePermissions` for the owner: `OWNER` (4) when
/// commands are allowed, else `ALL` (0).
fn owner_permission(allow_commands: bool) -> u8 {
    if allow_commands { 4 } else { 0 }
}
