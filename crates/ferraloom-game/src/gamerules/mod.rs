//! Port of net.minecraft.world.level.gamerules. Rules are server saved data in 26.3.
mod registry;
use ferraloom_brigadier::{
    ArgumentType, BoolArgumentType, CommandSyntaxException, IntegerArgumentType, StringReader,
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_storage::{Error, SavedDataCodec};
pub use registry::{GameRule, GameRuleDefinition};
use std::fmt;

/// Port of GameRuleCategory's built-in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleCategory {
    Player,
    Mobs,
    Spawning,
    Drops,
    Updates,
    Chat,
    Misc,
}
impl GameRuleCategory {
    pub fn id(self) -> &'static str {
        match self {
            Self::Player => "minecraft:player",
            Self::Mobs => "minecraft:mobs",
            Self::Spawning => "minecraft:spawning",
            Self::Drops => "minecraft:drops",
            Self::Updates => "minecraft:updates",
            Self::Chat => "minecraft:chat",
            Self::Misc => "minecraft:misc",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleType {
    Boolean { default: bool },
    Integer { default: i32, min: i32, max: i32 },
}
impl RuleType {
    pub fn default_value(self) -> GameRuleValue {
        match self {
            Self::Boolean { default } => GameRuleValue::Boolean(default),
            Self::Integer { default, .. } => GameRuleValue::Integer(default),
        }
    }
    /// Port of GameRule#argument. The reader retains Brigadier's error and cursor.
    pub fn parse(self, reader: &mut StringReader) -> Result<GameRuleValue, CommandSyntaxException> {
        match self {
            Self::Boolean { .. } => BoolArgumentType.parse(reader).map(GameRuleValue::Boolean),
            Self::Integer { min, max, .. } => IntegerArgumentType::new(min, max)
                .parse(reader)
                .map(GameRuleValue::Integer),
        }
    }
    pub fn accepts(self, value: GameRuleValue) -> bool {
        match (self, value) {
            (Self::Boolean { .. }, GameRuleValue::Boolean(_)) => true,
            (Self::Integer { min, max, .. }, GameRuleValue::Integer(value)) => {
                (min..=max).contains(&value)
            }
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleValue {
    Boolean(bool),
    Integer(i32),
}
impl GameRuleValue {
    pub fn command_result(self) -> i32 {
        match self {
            Self::Boolean(value) => i32::from(value),
            Self::Integer(value) => value,
        }
    }
}
impl fmt::Display for GameRuleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(value) => value.fmt(f),
            Self::Integer(value) => value.fmt(f),
        }
    }
}
/// Port of GameRule#deserialize, including DataResult's partial trailing-data value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeserializeError {
    Invalid,
    TrailingCharacters(GameRuleValue),
}
impl GameRule {
    pub fn deserialize(self, input: &str) -> Result<GameRuleValue, DeserializeError> {
        let mut reader = StringReader::new(input);
        let value = self
            .definition()
            .kind
            .parse(&mut reader)
            .map_err(|_| DeserializeError::Invalid)?;
        if reader.can_read() {
            Err(DeserializeError::TrailingCharacters(value))
        } else {
            Ok(value)
        }
    }
}
/// Feature flags relevant to built-in rules; no rule is conditionally enabled by anything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuleFeatures {
    pub minecart_improvements: bool,
}

/// Port of GameRuleMap. Missing entries are allowed until GameRules applies feature gates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRuleMap {
    values: [Option<GameRuleValue>; GameRule::ALL.len()],
}
impl Default for GameRuleMap {
    fn default() -> Self {
        Self {
            values: [None; GameRule::ALL.len()],
        }
    }
}
impl GameRuleMap {
    pub const STORAGE_ID: &'static str = "minecraft:game_rules";
    pub fn get(&self, rule: GameRule) -> Option<GameRuleValue> {
        self.values[rule as usize]
    }
    pub fn iter(&self) -> impl Iterator<Item = (GameRule, GameRuleValue)> + '_ {
        GameRule::ALL
            .into_iter()
            .filter_map(|rule| self.get(rule).map(|value| (rule, value)))
    }
}
/// DataResult may retain a partial rule map alongside a codec error.
#[derive(Clone, Debug)]
pub struct GameRuleMapResult {
    pub value: Option<GameRuleMap>,
    pub error: Option<String>,
}
impl GameRuleMap {
    /// Port of GameRuleMap.CODEC's dispatched-map decode, including missed entries.
    pub fn decode_partial(tag: &Tag) -> GameRuleMapResult {
        let Some(compound) = tag.as_compound() else {
            return GameRuleMapResult {
                value: None,
                error: Some(format!("Not a map: {tag}")),
            };
        };
        let mut map = Self::default();
        let mut errors = Vec::new();
        let mut missed = CompoundTag::new();
        for (key, value) in compound.iter_java_order() {
            let rule = match saved_rule_key(&key.to_string_lossy()) {
                Ok(rule) => rule,
                Err(message) => {
                    // DispatchedMapCodec reports both key decode and codec lookup errors.
                    errors.push(message.clone());
                    errors.push(message);
                    missed.insert(key.clone(), value.clone());
                    continue;
                }
            };
            match decode_rule_value(rule, value) {
                Ok(value) => map.values[rule as usize] = Some(value),
                Err(message) => {
                    errors.push(message);
                    missed.insert(key.clone(), value.clone());
                }
            }
        }
        // DispatchedMapCodec's applicative accumulation prepends each entry's errors.
        errors.reverse();
        let error =
            (!errors.is_empty()).then(|| format!("{} missed input: {missed}", errors.join("; ")));
        GameRuleMapResult {
            value: Some(map),
            error,
        }
    }
}
/// Port of Identifier.CODEC followed by the game-rule registry's by-name lookup.
fn saved_rule_key(key: &str) -> Result<GameRule, String> {
    let id = ferraloom_protocol::Identifier::parse(key).map_err(|_| {
        let (namespace, path) = key.split_once(':').unwrap_or(("minecraft", key));
        let namespace = if namespace.is_empty() {
            "minecraft"
        } else {
            namespace
        };
        let namespace_valid = namespace
            .bytes()
            .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'));
        let detail = if !namespace_valid {
            format!("Non [a-z0-9_.-] character in namespace of identifier: {namespace}:{path}")
        } else {
            format!("Non [a-z0-9/._-] character in path of location: {namespace}:{path}")
        };
        format!("Not a valid resource location: {key} {detail}")
    })?;
    GameRule::by_id(id.as_str()).ok_or_else(|| {
        format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:game_rule]: {id}")
    })
}
fn decode_rule_value(rule: GameRule, tag: &Tag) -> Result<GameRuleValue, String> {
    match rule.definition().kind {
        RuleType::Boolean { .. } => Ok(GameRuleValue::Boolean(
            tag.as_double().ok_or("Not a number")? != 0.,
        )),
        RuleType::Integer { min, max, .. } => {
            let value = match tag {
                Tag::Byte(value) => i32::from(*value),
                Tag::Short(value) => i32::from(*value),
                Tag::Int(value) => *value,
                Tag::Long(value) => *value as i32,
                Tag::Float(value) => *value as i32,
                Tag::Double(value) => *value as i32,
                _ => return Err("Not a number".into()),
            };
            if !(min..=max).contains(&value) {
                return Err(format!("Value {value} outside of range [{min}:{max}]"));
            }
            Ok(GameRuleValue::Integer(value))
        }
    }
}
impl SavedDataCodec for GameRuleMap {
    fn decode(tag: &Tag) -> ferraloom_storage::Result<Self> {
        let result = Self::decode_partial(tag);
        if let Some(error) = result.error {
            return Err(Error::Invalid(error));
        }
        Ok(result.value.expect("successful rule map codec has a value"))
    }
    fn encode(&self) -> ferraloom_storage::Result<Tag> {
        let mut tag = CompoundTag::new();
        for (rule, value) in self.iter() {
            if let (RuleType::Integer { min, max, .. }, GameRuleValue::Integer(value)) =
                (rule.definition().kind, value)
                && !(min..=max).contains(&value)
            {
                return Err(Error::Invalid(format!(
                    "Value {value} outside of range [{min}:{max}]"
                )));
            }
            tag.insert(
                rule.definition().id,
                match value {
                    GameRuleValue::Boolean(value) => Tag::Byte(i8::from(value)),
                    GameRuleValue::Integer(value) => Tag::Int(value),
                },
            );
        }
        Ok(Tag::Compound(tag))
    }
}
/// Port of MinecraftServer#onGameRuleChanged. The runtime applies effects synchronously.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameRuleEffect {
    ReducedDebugInfo { event: u8 },
    GameEvent { event: u8, enabled: bool },
    LocatorBar { enabled: bool },
    UpdateMobSpawningFlags,
    SynchronizeClocks,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRuleChange {
    pub rule: GameRule,
    pub value: GameRuleValue,
    pub effect: Option<GameRuleEffect>,
}
impl GameRuleChange {
    fn new(rule: GameRule, value: GameRuleValue) -> Self {
        use GameRule::*;
        let enabled = value == GameRuleValue::Boolean(true);
        let effect = match rule {
            ReducedDebugInfo => Some(GameRuleEffect::ReducedDebugInfo {
                event: if enabled { 22 } else { 23 },
            }),
            ImmediateRespawn => Some(GameRuleEffect::GameEvent { event: 11, enabled }),
            LimitedCrafting => Some(GameRuleEffect::GameEvent { event: 12, enabled }),
            LocatorBar => Some(GameRuleEffect::LocatorBar { enabled }),
            SpawnMonsters => Some(GameRuleEffect::UpdateMobSpawningFlags),
            AdvanceTime => Some(GameRuleEffect::SynchronizeClocks),
            _ => None,
        };
        Self {
            rule,
            value,
            effect,
        }
    }
}
/// Port of GameRules. Mutations return callbacks even when a value is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRules {
    map: GameRuleMap,
}
impl Default for GameRules {
    fn default() -> Self {
        Self::new(RuleFeatures::default(), GameRuleMap::default())
    }
}
impl GameRules {
    pub fn new(features: RuleFeatures, mut map: GameRuleMap) -> Self {
        for rule in GameRule::ALL {
            let definition = rule.definition();
            let value = &mut map.values[rule as usize];
            if definition.minecart_improvements && !features.minecart_improvements {
                *value = None;
            } else if value.is_none() {
                *value = Some(definition.kind.default_value());
            }
        }
        Self { map }
    }
    pub fn map(&self) -> &GameRuleMap {
        &self.map
    }
    pub fn get(&self, rule: GameRule) -> Option<GameRuleValue> {
        self.map.get(rule)
    }
    pub fn boolean(&self, rule: GameRule) -> bool {
        match self.get(rule) {
            Some(GameRuleValue::Boolean(value)) => value,
            _ => panic!("Tried to access invalid game rule {}", rule.definition().id),
        }
    }
    pub fn integer(&self, rule: GameRule) -> i32 {
        match self.get(rule) {
            Some(GameRuleValue::Integer(value)) => value,
            _ => panic!("Tried to access invalid game rule {}", rule.definition().id),
        }
    }
    pub fn set(
        &mut self,
        rule: GameRule,
        value: GameRuleValue,
    ) -> Result<GameRuleChange, InvalidGameRule> {
        // GameRules#set is a typed assignment. Bounds belong to the command argument
        // and the saved-data codec, including their encode boundary.
        let matching_type = matches!(
            (rule.definition().kind, value),
            (RuleType::Boolean { .. }, GameRuleValue::Boolean(_))
                | (RuleType::Integer { .. }, GameRuleValue::Integer(_))
        );
        if self.get(rule).is_none() || !matching_type {
            return Err(InvalidGameRule { rule, value });
        }
        self.map.values[rule as usize] = Some(value);
        Ok(GameRuleChange::new(rule, value))
    }
    pub fn set_all(&mut self, other: &GameRuleMap, mut on_change: impl FnMut(GameRuleChange)) {
        for (rule, value) in other.iter() {
            if let Ok(change) = self.set(rule, value) {
                on_change(change);
            }
        }
    }
    /// Supplies the live level's exact rule fields after load and every mutation,
    /// before callbacks or another command can query the simulation.
    pub fn update_level_rules(&self, rules: &mut crate::level::LevelRules) {
        rules.block_drops = Some(self.boolean(GameRule::BlockDrops));
        rules.spread_vines = self.boolean(GameRule::SpreadVines);
        rules.mob_griefing = self.boolean(GameRule::MobGriefing);
        rules.max_snow_accumulation_height = self.integer(GameRule::MaxSnowAccumulationHeight);
        rules.show_advancement_messages = self.boolean(GameRule::ShowAdvancementMessages);
        rules.spectators_generate_chunks = self.boolean(GameRule::SpectatorsGenerateChunks);
        rules.locator_bar = self.boolean(GameRule::LocatorBar);
        rules.natural_health_regeneration = self.boolean(GameRule::NaturalHealthRegeneration);
        rules.tnt_explodes = self.boolean(GameRule::TntExplodes);
        rules.entity_drops = self.boolean(GameRule::EntityDrops);
        rules.max_entity_cramming = self.integer(GameRule::MaxEntityCramming);
        rules.drowning_damage = self.boolean(GameRule::DrowningDamage);
        rules.fall_damage = self.boolean(GameRule::FallDamage);
        rules.fire_damage = self.boolean(GameRule::FireDamage);
        rules.freeze_damage = self.boolean(GameRule::FreezeDamage);
        rules.pvp = self.boolean(GameRule::Pvp);
        rules.mob_drops = self.boolean(GameRule::MobDrops);
        rules.keep_inventory = self.boolean(GameRule::KeepInventory);
        rules.limited_crafting = self.boolean(GameRule::LimitedCrafting);
        rules.random_tick_speed = self.integer(GameRule::RandomTickSpeed).max(0) as u32;
        rules.water_source_conversion = self.boolean(GameRule::WaterSourceConversion);
        rules.lava_source_conversion = self.boolean(GameRule::LavaSourceConversion);
        rules.send_command_feedback = self.boolean(GameRule::SendCommandFeedback);
        rules.players_nether_portal_default_delay =
            self.integer(GameRule::PlayersNetherPortalDefaultDelay);
    }
    pub fn copy(&self, features: RuleFeatures) -> Self {
        Self::new(features, self.map.clone())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidGameRule {
    pub rule: GameRule,
    pub value: GameRuleValue,
}
impl fmt::Display for InvalidGameRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Tried to set invalid game rule '{}' to value '{}'",
            self.rule.definition().id,
            self.value
        )
    }
}
impl std::error::Error for InvalidGameRule {}

#[cfg(test)]
mod tests;
