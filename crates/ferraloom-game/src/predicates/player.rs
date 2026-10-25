use super::{DoubleBounds, EntityPredicate, IntBounds, PredicateContext};
use crate::loot::codec::DecodeInput;
use crate::{
    entity::EntityRef,
    loot::{Identifier, Result, codec},
};
use ferraloom_protocol::play::GameType;
use std::collections::BTreeMap;

/// Port of FoodPredicate#matches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoodPredicate {
    pub level: IntBounds,
    pub saturation: DoubleBounds,
}
impl FoodPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            level: codec::optional(v, "level", IntBounds::decode)?.unwrap_or_default(),
            saturation: codec::optional(v, "saturation", DoubleBounds::decode)?.unwrap_or_default(),
        })
    }
    pub fn matches(&self, level: i32, saturation: f32) -> bool {
        self.level.matches(level) && self.saturation.matches(f64::from(saturation))
    }
}
/// Port of InputPredicate's seven ordered flags (sneak reads Input#shift).
#[derive(Clone, Debug, PartialEq)]
pub struct InputPredicate(pub [Option<bool>; 7]);
impl InputPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let mut flags = [None; 7];
        for (i, key) in [
            "forward", "backward", "left", "right", "jump", "sneak", "sprint",
        ]
        .iter()
        .enumerate()
        {
            flags[i] = codec::optional(v, key, codec::boolean)?;
        }
        Ok(Self(flags))
    }
    pub fn matches(&self, input: [bool; 7]) -> bool {
        self.0
            .iter()
            .zip(input)
            .all(|(test, actual)| test.is_none_or(|test| test == actual))
    }
}
/// Port of GameTypePredicate#CODEC. The codec accepts a list, including empty.
#[derive(Clone, Debug, PartialEq)]
pub struct GameTypePredicate(pub Vec<GameType>);
impl GameTypePredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self(codec::list(v, |v| {
            Ok(match codec::string(v)? {
                "survival" => GameType::Survival,
                "creative" => GameType::Creative,
                "adventure" => GameType::Adventure,
                "spectator" => GameType::Spectator,
                _ => return Err(codec::error("unknown game type")),
            })
        })?))
    }
    pub fn matches(&self, value: GameType) -> bool {
        self.0.contains(&value)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StatMatcher {
    pub stat_type: Identifier,
    pub stat: Identifier,
    pub value: IntBounds,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AdvancementPredicate {
    Done(bool),
    Criteria(Vec<(String, bool)>),
}
#[derive(Clone, Debug)]
pub struct AdvancementFacts {
    pub done: bool,
    pub criteria: BTreeMap<String, bool>,
}
/// Port of PlayerPredicate's server-player gate and ordered live queries.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerPredicate {
    pub level: IntBounds,
    pub food: FoodPredicate,
    pub gamemode: Option<GameTypePredicate>,
    pub stats: Vec<StatMatcher>,
    pub recipes: Vec<(Identifier, bool)>,
    pub advancements: Vec<(Identifier, AdvancementPredicate)>,
    pub looking_at: Option<Box<EntityPredicate>>,
    pub input: Option<InputPredicate>,
}
impl PlayerPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            level: codec::optional(v, "level", IntBounds::decode)?.unwrap_or_default(),
            food: codec::optional(v, "food", FoodPredicate::decode)?.unwrap_or_default(),
            gamemode: codec::optional(v, "gamemode", GameTypePredicate::decode)?,
            stats: codec::optional(v, "stats", |v| {
                codec::list(v, |v| {
                    let stat_type = codec::id(codec::field(v, "type")?)?;
                    let stat = codec::id(codec::field(v, "stat")?)?;
                    let registry = match stat_type.as_str() {
                        "minecraft:mined" => "minecraft:block",
                        "minecraft:crafted"
                        | "minecraft:used"
                        | "minecraft:broken"
                        | "minecraft:picked_up"
                        | "minecraft:dropped" => "minecraft:item",
                        "minecraft:killed" | "minecraft:killed_by" => "minecraft:entity_type",
                        "minecraft:custom" => "minecraft:custom_stat",
                        _ => return Err(codec::error("unknown stat type")),
                    };
                    if !ferraloom_data::registry_entries(registry)
                        .is_some_and(|entries| entries.contains(&stat.as_str()))
                    {
                        return Err(codec::error("unknown stat"));
                    }
                    Ok(StatMatcher {
                        stat_type,
                        stat,
                        value: codec::optional(v, "value", IntBounds::decode)?.unwrap_or_default(),
                    })
                })
            })?
            .unwrap_or_default(),
            recipes: codec::optional(v, "recipes", |v| {
                codec::map(v)?
                    .entries()?
                    .into_iter()
                    .map(|(key, value)| Ok((Identifier::parse(key)?, codec::boolean(value)?)))
                    .collect::<Result<_>>()
            })?
            .unwrap_or_default(),
            advancements: codec::optional(v, "advancements", |v| {
                codec::map(v)?
                    .entries()?
                    .into_iter()
                    .map(|(key, value)| {
                        let test = if value.is_boolean() {
                            AdvancementPredicate::Done(codec::boolean(value)?)
                        } else {
                            AdvancementPredicate::Criteria(
                                codec::map(value)?
                                    .entries()?
                                    .into_iter()
                                    .map(|(key, value)| {
                                        Ok((key.to_owned(), codec::boolean(value)?))
                                    })
                                    .collect::<Result<_>>()?,
                            )
                        };
                        Ok((Identifier::parse(key)?, test))
                    })
                    .collect::<Result<_>>()
            })?
            .unwrap_or_default(),
            looking_at: codec::optional(v, "looking_at", |v| {
                Ok(Box::new(EntityPredicate::decode(v)?))
            })?,
            input: codec::optional(v, "input", InputPredicate::decode)?,
        })
    }
    pub fn matches(&self, entity: EntityRef, facts: &dyn PredicateContext) -> Result<bool> {
        let Some(level) = facts.player_experience_level(entity)? else {
            return Ok(false);
        };
        if !self.level.matches(level) {
            return Ok(false);
        }
        // Unconstrained bounds always match; native food getters have no side effects.
        if self.food != FoodPredicate::default() {
            let (food, saturation) = facts.player_food(entity)?;
            if !self.food.matches(food, saturation) {
                return Ok(false);
            }
        }
        if let Some(test) = &self.gamemode
            && !test.matches(facts.player_gamemode(entity)?)
        {
            return Ok(false);
        }
        for stat in &self.stats {
            if !stat
                .value
                .matches(facts.player_stat(entity, &stat.stat_type, &stat.stat)?)
            {
                return Ok(false);
            }
        }
        for (recipe, expected) in &self.recipes {
            if facts.player_recipe(entity, recipe)? != *expected {
                return Ok(false);
            }
        }
        for (id, test) in &self.advancements {
            let Some(progress) = facts.player_advancement(entity, id)? else {
                return Ok(false);
            };
            let matches = match test {
                AdvancementPredicate::Done(expected) => *expected == progress.done,
                AdvancementPredicate::Criteria(tests) => tests
                    .iter()
                    .all(|(key, expected)| progress.criteria.get(key) == Some(expected)),
            };
            if !matches {
                return Ok(false);
            }
        }
        if let Some(test) = &self.looking_at {
            let Some(target) = facts.player_looking_at(entity)? else {
                return Ok(false);
            };
            if !test.matches(Some(target), Some(facts.entity_position(entity)?), facts)?
                || !facts.player_has_line_of_sight(entity, target)?
            {
                return Ok(false);
            }
        }
        if let Some(test) = &self.input
            && !test.matches(facts.player_input(entity)?)
        {
            return Ok(false);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct WithoutFood;
    impl PredicateContext for WithoutFood {
        fn player_experience_level(&self, _: EntityRef) -> Result<Option<i32>> {
            Ok(Some(0))
        }
    }
    #[test]
    fn unconstrained_food_needs_no_unavailable_state() {
        let empty = PlayerPredicate::decode(&serde_json::json!({})).unwrap();
        assert!(
            empty
                .matches(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                    &WithoutFood
                )
                .unwrap()
        );
        let bounded = PlayerPredicate::decode(&serde_json::json!({"food":{"level":20}})).unwrap();
        assert!(matches!(
            bounded.matches(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                &WithoutFood
            ),
            Err(crate::loot::LootError::Unsupported(_))
        ));
    }
}
