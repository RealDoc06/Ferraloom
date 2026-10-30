//! Game rules (26.3 `GameRules`): the registered rules with their category,
//! type, default and bounds, a value map, and persistence in the world's
//! `data/minecraft/game_rules.dat` saved-data file.
use horizon_world::{
    DATA_VERSION,
    compression::Compression,
    nbt::{self, Limits, NamedTag, Tag},
};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i32),
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
        }
    }
}
pub struct Rule {
    pub name: &'static str,
    pub category: &'static str,
    pub default: Value,
    pub min: i32,
    pub max: i32,
}
const fn b(name: &'static str, category: &'static str, default: bool) -> Rule {
    Rule {
        name,
        category,
        default: Value::Bool(default),
        min: 0,
        max: 0,
    }
}
const fn i(name: &'static str, category: &'static str, default: i32, min: i32, max: i32) -> Rule {
    Rule {
        name,
        category,
        default: Value::Int(default),
        min,
        max,
    }
}
/// Every rule available without experimental features, in registration order.
pub const RULES: &[Rule] = &[
    b("advance_time", "updates", true),
    b("advance_weather", "updates", true),
    b("allow_entering_nether_using_portals", "misc", true),
    b("block_drops", "drops", true),
    b("block_explosion_drop_decay", "drops", true),
    b("command_blocks_work", "misc", true),
    b("command_block_output", "chat", true),
    b("drowning_damage", "player", true),
    b("elytra_movement_check", "player", true),
    b("ender_pearls_vanish_on_death", "player", true),
    b("entity_drops", "drops", true),
    b("fall_damage", "player", true),
    b("fire_damage", "player", true),
    i(
        "fire_spread_radius_around_player",
        "updates",
        128,
        -1,
        i32::MAX,
    ),
    b("forgive_dead_players", "mobs", true),
    b("freeze_damage", "player", true),
    b("global_sound_events", "misc", true),
    b("immediate_respawn", "player", false),
    b("keep_inventory", "player", false),
    b("lava_source_conversion", "updates", false),
    b("limited_crafting", "player", false),
    b("locator_bar", "player", true),
    b("log_admin_commands", "chat", true),
    i("max_block_modifications", "misc", 32768, 1, i32::MAX),
    i("max_command_forks", "misc", 65536, 0, i32::MAX),
    i("max_command_sequence_length", "misc", 65536, 0, i32::MAX),
    i("max_entity_cramming", "mobs", 24, 0, i32::MAX),
    i("max_snow_accumulation_height", "updates", 1, 0, 8),
    b("mob_drops", "drops", true),
    b("mob_explosion_drop_decay", "drops", true),
    b("mob_griefing", "mobs", true),
    b("natural_health_regeneration", "player", true),
    b("player_movement_check", "player", true),
    i(
        "players_nether_portal_creative_delay",
        "player",
        0,
        0,
        i32::MAX,
    ),
    i(
        "players_nether_portal_default_delay",
        "player",
        80,
        0,
        i32::MAX,
    ),
    i("players_sleeping_percentage", "player", 100, 0, i32::MAX),
    b("projectiles_can_break_blocks", "drops", true),
    b("pvp", "player", true),
    b("raids", "mobs", true),
    i("random_tick_speed", "updates", 3, 0, i32::MAX),
    b("reduced_debug_info", "misc", false),
    i("respawn_radius", "player", 10, 0, i32::MAX),
    b("send_command_feedback", "chat", true),
    b("show_advancement_messages", "chat", true),
    b("show_death_messages", "chat", true),
    b("spawner_blocks_work", "misc", true),
    b("spawn_mobs", "spawning", true),
    b("spawn_monsters", "spawning", true),
    b("spawn_patrols", "spawning", true),
    b("spawn_phantoms", "spawning", true),
    b("spawn_wandering_traders", "spawning", true),
    b("spawn_wardens", "spawning", true),
    b("spectators_generate_chunks", "player", true),
    b("spread_vines", "updates", true),
    b("tnt_explodes", "misc", true),
    b("tnt_explosion_drop_decay", "drops", false),
    b("universal_anger", "mobs", false),
    b("water_source_conversion", "updates", true),
];
/// A resolved registration index for repeated lookups. Never persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleId(usize);

pub fn rule(name: &str) -> Option<&'static Rule> {
    rule_id(name).map(|id| &RULES[id.0])
}

/// Resolve a name at setup; retain the handle for ticking code.
pub fn rule_id(name: &str) -> Option<RuleId> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    RULES.iter().position(|rule| rule.name == name).map(RuleId)
}

/// Current values (defaults unless set).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameRules {
    values: BTreeMap<RuleId, Value>,
}
impl GameRules {
    pub fn get(&self, name: &str) -> Option<Value> {
        rule_id(name).map(|id| self.get_by_id(id))
    }
    /// Read current values through a handle without comparing names.
    pub fn get_by_id(&self, id: RuleId) -> Value {
        self.values.get(&id).copied().unwrap_or(RULES[id.0].default)
    }
    pub fn bool_by_id(&self, id: RuleId) -> bool {
        matches!(self.get_by_id(id), Value::Bool(true))
    }
    pub fn int_by_id(&self, id: RuleId) -> i32 {
        match self.get_by_id(id) {
            Value::Int(value) => value,
            _ => 0,
        }
    }
    pub fn bool(&self, name: &str) -> bool {
        matches!(self.get(name), Some(Value::Bool(true)))
    }
    /// Parse a network rule value, retaining type and range validation.
    pub fn set_string(&mut self, name: &str, text: &str) -> bool {
        let value = match self.get(name) {
            Some(Value::Bool(_)) => text.parse::<bool>().ok().map(Value::Bool),
            Some(Value::Int(_)) => text.parse::<i32>().ok().map(Value::Int),
            None => None,
        };
        value.is_some_and(|v| self.set(name, v))
    }
    pub fn int(&self, name: &str) -> i32 {
        match self.get(name) {
            Some(Value::Int(v)) => v,
            _ => 0,
        }
    }
    /// Set a rule; false if the name or value type is wrong.
    pub fn set(&mut self, name: &str, value: Value) -> bool {
        let Some(id) = rule_id(name) else {
            return false;
        };
        let r = &RULES[id.0];
        let ok = match (r.default, value) {
            (Value::Bool(_), Value::Bool(_)) => true,
            (Value::Int(_), Value::Int(v)) => (r.min..=r.max).contains(&v),
            _ => false,
        };
        if ok && value == r.default {
            self.values.remove(&id);
        } else if ok {
            self.values.insert(id, value);
        }
        ok
    }
    /// Every rule's value by name without namespace, as `save::Level` holds
    /// them (booleans as 0 or 1).
    pub fn by_name(&self) -> BTreeMap<String, i32> {
        RULES
            .iter()
            .enumerate()
            .map(|(id, r)| {
                let v = match self.get_by_id(RuleId(id)) {
                    Value::Bool(b) => i32::from(b),
                    Value::Int(i) => i,
                };
                (r.name.to_owned(), v)
            })
            .collect()
    }
    /// GameRuleMap codec: `{data: {"minecraft:<rule>": value...}, DataVersion}`.
    pub fn to_nbt(&self) -> NamedTag {
        let data: Vec<(horizon_world::nbt::JavaString, Tag)> = RULES
            .iter()
            .enumerate()
            .map(|(id, r)| {
                let v = self.get_by_id(RuleId(id));
                let tag = match v {
                    Value::Bool(b) => Tag::Byte(i8::from(b)),
                    Value::Int(i) => Tag::Int(i),
                };
                (
                    horizon_world::nbt::JavaString::from(format!("minecraft:{}", r.name).as_str()),
                    tag,
                )
            })
            .collect();
        NamedTag {
            name: "".into(),
            tag: Tag::compound([
                ("data", Tag::compound(data)),
                ("DataVersion", Tag::Int(DATA_VERSION)),
            ]),
        }
    }
    pub fn from_nbt(root: &Tag) -> Self {
        let mut rules = Self::default();
        let Some(data) = root.get("data") else {
            return rules;
        };
        for r in RULES {
            let key = format!("minecraft:{}", r.name);
            let value = data.get(&key).or_else(|| data.get(r.name));
            match (r.default, value) {
                (Value::Bool(_), Some(Tag::Byte(v))) => {
                    rules.set(r.name, Value::Bool(*v != 0));
                }
                (Value::Int(_), Some(Tag::Int(v))) => {
                    rules.set(r.name, Value::Int(*v));
                }
                _ => {}
            }
        }
        rules
    }
    pub fn load(world: &Path) -> Self {
        let Ok(bytes) = fs::read(world.join("data/minecraft/game_rules.dat")) else {
            return Self::default();
        };
        let Ok(raw) = Compression::Gzip.decode(&bytes, 16 << 20) else {
            return Self::default();
        };
        match nbt::read_named(&raw, Limits::default()) {
            Ok((root, _)) => Self::from_nbt(&root.tag),
            Err(_) => Self::default(),
        }
    }
    pub fn save(&self, world: &Path) -> crate::Result<()> {
        let dir = world.join("data/minecraft");
        fs::create_dir_all(&dir)?;
        let bytes =
            nbt::write_named(&self.to_nbt(), Limits::default()).map_err(|e| e.to_string())?;
        fs::write(
            dir.join("game_rules.dat"),
            Compression::Gzip.encode(&bytes)?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_rules_keep_registration_identity_and_overrides() {
        let mut rules = GameRules::default();
        for (id, r) in RULES.iter().enumerate() {
            assert_eq!(rule_id(r.name), Some(RuleId(id)));
            assert_eq!(rule(r.name).unwrap().name, r.name);
            let namespaced = format!("minecraft:{}", r.name);
            assert_eq!(rules.get(&namespaced), Some(r.default));
            let changed = match r.default {
                Value::Bool(v) => Value::Bool(!v),
                Value::Int(v) => Value::Int(if v < r.max { v + 1 } else { v - 1 }),
            };
            assert!(rules.set(&namespaced, changed));
            assert_eq!(rules.get(r.name), Some(changed));
            assert_eq!(rules.get_by_id(RuleId(id)), changed);
        }
        assert_eq!(GameRules::from_nbt(&rules.to_nbt().tag), rules);
        for r in RULES {
            assert!(rules.set(r.name, r.default));
        }
        assert_eq!(rules, GameRules::default());
    }

    /// Run with `cargo test --release -p horizon-client-core benchmark_rule_lookup -- --ignored --nocapture`.
    #[test]
    #[ignore = "release micro-benchmark"]
    fn benchmark_rule_lookup() {
        use std::{hint::black_box, time::Instant};
        // The nine lookups in Game::level_rules, on every adapter construction.
        let names = [
            "water_source_conversion",
            "lava_source_conversion",
            "random_tick_speed",
            "block_drops",
            "mob_griefing",
            "max_snow_accumulation_height",
            "spawn_mobs",
            "spawn_monsters",
            "fire_spread_radius_around_player",
        ];
        let ids = names.map(|name| rule_id(name).unwrap());
        for overrides in [false, true] {
            let mut rules = GameRules::default();
            let mut old = BTreeMap::new();
            if overrides {
                for r in RULES {
                    let value = match r.default {
                        Value::Bool(v) => Value::Bool(!v),
                        Value::Int(v) => Value::Int(v + 1),
                    };
                    if rules.set(r.name, value) {
                        old.insert(r.name, value);
                    }
                }
            }
            for (name, id) in names.into_iter().zip(ids) {
                let r = RULES.iter().find(|r| r.name == name).unwrap();
                let expected = old.get(r.name).copied().unwrap_or(r.default);
                assert_eq!(rules.get(name), Some(expected));
                assert_eq!(rules.get_by_id(id), expected);
            }
            for round in 0..3 {
                let start = Instant::now();
                for _ in 0..200_000 {
                    for name in black_box(names) {
                        let name = black_box(name);
                        let r = RULES.iter().find(|r| r.name == name).unwrap();
                        black_box(old.get(r.name).copied().unwrap_or(r.default));
                    }
                }
                let before = start.elapsed();
                let start = Instant::now();
                for _ in 0..200_000 {
                    for id in black_box(ids) {
                        black_box(rules.get_by_id(black_box(id)));
                    }
                }
                println!(
                    "rules overrides={overrides} round={round}: old={before:?} numeric={:?}",
                    start.elapsed()
                );
            }
        }
    }

    #[test]
    fn network_values_validate_rule_names_types_and_bounds() {
        let mut rules = GameRules::default();
        assert!(rules.set_string("minecraft:keep_inventory", "true"));
        assert!(rules.bool("keep_inventory"));
        assert!(!rules.set_string("minecraft:keep_inventory", "1"));
        assert!(rules.bool("keep_inventory"));
        assert!(rules.set_string("minecraft:random_tick_speed", "7"));
        assert_eq!(rules.int("random_tick_speed"), 7);
        assert!(!rules.set_string("minecraft:random_tick_speed", "-1"));
        assert!(!rules.set_string("minecraft:random_tick_speed", "999999999999"));
        assert!(!rules.set_string("minecraft:unknown", "true"));
        assert!(!rules.set_string("other:keep_inventory", "false"));
        assert_eq!(rules.int("random_tick_speed"), 7);
    }
    #[test]
    fn defaults_bounds_and_round_trip() {
        let mut rules = GameRules::default();
        assert!(rules.bool("advance_time"));
        assert_eq!(rules.int("random_tick_speed"), 3);
        assert!(!rules.set("random_tick_speed", Value::Bool(true)));
        assert!(!rules.set("max_snow_accumulation_height", Value::Int(9)));
        assert!(rules.set("keep_inventory", Value::Bool(true)));
        assert!(rules.set("minecraft:random_tick_speed", Value::Int(10)));
        let back = GameRules::from_nbt(&rules.to_nbt().tag);
        assert_eq!(back, rules);
        assert!(back.bool("keep_inventory"));
        let dir = std::env::temp_dir().join(format!("horizon-gamerules-{}", std::process::id()));
        rules.save(&dir).unwrap();
        assert_eq!(GameRules::load(&dir), rules);
        let _ = fs::remove_dir_all(dir);
    }
}
