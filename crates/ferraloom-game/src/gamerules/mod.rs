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
impl SavedDataCodec for GameRuleMap {
    fn decode(tag: &Tag) -> ferraloom_storage::Result<Self> {
        let compound = tag
            .as_compound()
            .ok_or_else(|| Error::Invalid("game rules are not a compound".into()))?;
        let mut result = Self::default();
        for (id, tag) in compound.iter() {
            let id = id.to_string_lossy();
            let rule = GameRule::by_id(&id)
                .ok_or_else(|| Error::Invalid(format!("Unknown game rule: {id}")))?;
            let value = match rule.definition().kind {
                RuleType::Boolean { .. } => GameRuleValue::Boolean(
                    tag.as_double().ok_or_else(|| {
                        Error::Invalid(format!("Invalid value for game rule {id}"))
                    })? != 0.,
                ),
                RuleType::Integer { .. } => GameRuleValue::Integer(match tag {
                    Tag::Byte(value) => i32::from(*value),
                    Tag::Short(value) => i32::from(*value),
                    Tag::Int(value) => *value,
                    Tag::Long(value) => *value as i32,
                    Tag::Float(value) => *value as i32,
                    Tag::Double(value) => *value as i32,
                    _ => return Err(Error::Invalid(format!("Invalid value for game rule {id}"))),
                }),
            };
            if !rule.definition().kind.accepts(value) {
                return Err(Error::Invalid(format!(
                    "Game rule {id} out of range: {value}"
                )));
            }
            result.values[rule as usize] = Some(value);
        }
        Ok(result)
    }
    fn encode(&self) -> ferraloom_storage::Result<Tag> {
        let mut tag = CompoundTag::new();
        for (rule, value) in self.iter() {
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
            _ => panic!(
                "Tried to access invalid boolean game rule {}",
                rule.definition().id
            ),
        }
    }
    pub fn integer(&self, rule: GameRule) -> i32 {
        match self.get(rule) {
            Some(GameRuleValue::Integer(value)) => value,
            _ => panic!(
                "Tried to access invalid integer game rule {}",
                rule.definition().id
            ),
        }
    }
    pub fn set(
        &mut self,
        rule: GameRule,
        value: GameRuleValue,
    ) -> Result<GameRuleChange, InvalidGameRule> {
        if self.get(rule).is_none() || !rule.definition().kind.accepts(value) {
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
