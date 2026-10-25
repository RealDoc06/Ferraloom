use super::{
    ContextFloatProvider, ContextIntProvider, ContextParam, EntityTarget, EvalContext, Holder,
    HolderSet, LootData, LootError, Result, codec,
};
use crate::loot::codec::DecodeInput;
use crate::predicates::{BlockPredicate, EntityPredicate, ItemPredicate, LocationPredicate};
use ferraloom_util::Vec3;
use ferraloom_util::random::RandomSource;

/// Port of IntRangePredicate and FloatRangePredicate. An empty line deliberately
/// skips evaluating the tested provider, including any random draws it owns.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderRange<T> {
    Point(Holder<T>),
    Line {
        min: Option<Holder<T>>,
        max: Option<Holder<T>>,
    },
}
impl<T> ProviderRange<T> {
    fn decode<I: DecodeInput>(v: &I, decode: impl Fn(&I) -> Result<T> + Copy) -> Result<Self> {
        if let Ok(point) = Holder::decode(v, decode) {
            return Ok(Self::Point(point));
        }
        Ok(Self::Line {
            min: codec::optional(v, "min", |v| Holder::decode(v, decode))?,
            max: codec::optional(v, "max", |v| Holder::decode(v, decode))?,
        })
    }
    fn is_any(&self) -> bool {
        matches!(
            self,
            Self::Line {
                min: None,
                max: None
            }
        )
    }
}
macro_rules! range {
    ($ty:ty,$scalar:ty) => {
        impl ProviderRange<$ty> {
            pub(crate) fn test(
                &self,
                value: $scalar,
                data: &LootData,
                c: &mut EvalContext<'_>,
            ) -> Result<bool> {
                Ok(match self {
                    Self::Point(point) => point.get(data, c)? == value,
                    Self::Line { min, max } => {
                        if let Some(min) = min
                            && value < min.get(data, c)?
                        {
                            return Ok(false);
                        }
                        if let Some(max) = max
                            && value > max.get(data, c)?
                        {
                            return Ok(false);
                        }
                        true
                    }
                })
            }
        }
    };
}
range!(ContextIntProvider, i32);
range!(ContextFloatProvider, f32);
/// Port of LootItemCondition's 26.3 type-dispatched codec.
#[derive(Clone, Debug, PartialEq)]
pub enum LootItemCondition {
    Inverted(Holder<Self>),
    AllOf(HolderSet<Self>),
    AnyOf(HolderSet<Self>),
    RandomChance(Holder<ContextFloatProvider>),
    TableBonus(super::Identifier, Vec<f32>),
    RandomChanceWithEnchantedBonus {
        unenchanted: f32,
        enchanted: super::LevelBasedValue,
        enchantment: super::Identifier,
    },
    KilledByPlayer,
    SurvivesExplosion,
    MatchTool(Option<ItemPredicate>),
    MatchBlock(BlockPredicate),
    EntityProperties(EntityTarget, Option<EntityPredicate>),
    EntityScores(
        EntityTarget,
        Vec<(String, ProviderRange<ContextIntProvider>)>,
    ),
    LocationCheck(Option<Box<LocationPredicate>>, [i32; 3]),
    WeatherCheck {
        raining: Option<bool>,
        thundering: Option<bool>,
    },
    EnchantmentActive(bool),
    TimeCheck {
        clock: super::Identifier,
        period: Option<i64>,
        value: ProviderRange<ContextIntProvider>,
    },
    DamageSource(Option<crate::predicates::DamageSourcePredicate>),
    IntValueCheck(
        Holder<ContextIntProvider>,
        ProviderRange<ContextIntProvider>,
    ),
    FloatValueCheck(
        Holder<ContextFloatProvider>,
        ProviderRange<ContextFloatProvider>,
    ),
    EnvironmentAttribute {
        attribute: super::Identifier,
        value: super::EnvironmentValue,
        positional: bool,
    },
}
impl LootItemCondition {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let kind = codec::kind(v)?;
        Ok(match kind.as_str() {
            "minecraft:inverted" => {
                Self::Inverted(Holder::decode(codec::field(v, "term")?, Self::decode)?)
            }
            "minecraft:all_of" => {
                Self::AllOf(HolderSet::decode(codec::field(v, "terms")?, Self::decode)?)
            }
            "minecraft:any_of" => {
                Self::AnyOf(HolderSet::decode(codec::field(v, "terms")?, Self::decode)?)
            }
            "minecraft:random_chance" => Self::RandomChance(Holder::decode(
                codec::field(v, "chance")?,
                ContextFloatProvider::decode,
            )?),
            "minecraft:table_bonus" => {
                let chances = codec::list(codec::field(v, "chances")?, codec::float)?;
                if chances.is_empty() {
                    return Err(codec::error("empty chances"));
                }
                Self::TableBonus(codec::id(codec::field(v, "enchantment")?)?, chances)
            }
            "minecraft:random_chance_with_enchanted_bonus" => {
                let unenchanted = codec::float(codec::field(v, "unenchanted_chance")?)?;
                if !(0.0..=1.0).contains(&unenchanted) {
                    return Err(codec::error("chance outside [0,1]"));
                }
                Self::RandomChanceWithEnchantedBonus {
                    unenchanted,
                    enchanted: super::LevelBasedValue::decode(codec::field(
                        v,
                        "enchanted_chance",
                    )?)?,
                    enchantment: codec::id(codec::field(v, "enchantment")?)?,
                }
            }
            "minecraft:killed_by_player" => Self::KilledByPlayer,
            "minecraft:survives_explosion" => Self::SurvivesExplosion,
            "minecraft:match_tool" => {
                Self::MatchTool(codec::optional(v, "predicate", ItemPredicate::decode)?)
            }
            "minecraft:match_block" => Self::MatchBlock(BlockPredicate::decode(v)?),
            "minecraft:entity_properties" => Self::EntityProperties(
                EntityTarget::decode(codec::field(v, "entity")?)?,
                codec::optional(v, "predicate", EntityPredicate::decode)?,
            ),
            "minecraft:entity_scores" => Self::EntityScores(
                EntityTarget::decode(codec::field(v, "entity")?)?,
                codec::map(codec::field(v, "scores")?)?
                    .entries()?
                    .into_iter()
                    .map(|(key, value)| {
                        Ok((
                            key.to_owned(),
                            ProviderRange::decode(value, ContextIntProvider::decode)?,
                        ))
                    })
                    .collect::<Result<_>>()?,
            ),
            "minecraft:location_check" => Self::LocationCheck(
                codec::optional(v, "predicate", LocationPredicate::decode)?.map(Box::new),
                [
                    codec::optional(v, "offsetX", codec::int)?.unwrap_or(0),
                    codec::optional(v, "offsetY", codec::int)?.unwrap_or(0),
                    codec::optional(v, "offsetZ", codec::int)?.unwrap_or(0),
                ],
            ),
            "minecraft:weather_check" => Self::WeatherCheck {
                raining: codec::optional(v, "raining", codec::boolean)?,
                thundering: codec::optional(v, "thundering", codec::boolean)?,
            },
            "minecraft:damage_source_properties" => Self::DamageSource(codec::optional(
                v,
                "predicate",
                crate::predicates::DamageSourcePredicate::decode,
            )?),
            "minecraft:time_check" => Self::TimeCheck {
                clock: codec::id(codec::field(v, "clock")?)?,
                period: codec::optional(v, "period", codec::long)?,
                value: ProviderRange::decode(
                    codec::field(v, "value")?,
                    ContextIntProvider::decode,
                )?,
            },
            "minecraft:enchantment_active_check" => {
                Self::EnchantmentActive(codec::boolean(codec::field(v, "active")?)?)
            }
            "minecraft:int_value_check" => Self::IntValueCheck(
                Holder::decode(codec::field(v, "value")?, ContextIntProvider::decode)?,
                ProviderRange::decode(codec::field(v, "test")?, ContextIntProvider::decode)?,
            ),
            "minecraft:float_value_check" => Self::FloatValueCheck(
                Holder::decode(codec::field(v, "value")?, ContextFloatProvider::decode)?,
                ProviderRange::decode(codec::field(v, "test")?, ContextFloatProvider::decode)?,
            ),
            "minecraft:environment_attribute_check" => {
                let attribute = codec::id(codec::field(v, "attribute")?)?;
                let (value, positional) =
                    super::environment::check_value(&attribute, codec::field(v, "value")?)?;
                Self::EnvironmentAttribute {
                    attribute,
                    value,
                    positional,
                }
            }
            _ => return Err(codec::error(format!("unknown loot condition {kind}"))),
        })
    }
    pub(crate) fn test(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<bool> {
        match self {
            Self::Inverted(test) => Ok(!test.test(data, c)?),
            Self::AllOf(tests) => {
                for test in tests.resolve(data)? {
                    if !test.test(data, c)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::AnyOf(tests) => {
                for test in tests.resolve(data)? {
                    if test.test(data, c)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::RandomChance(chance) => {
                let chance = chance.get(data, c)?;
                Ok(c.random().next_float() < chance)
            }
            Self::TableBonus(enchantment, chances) => {
                let level = c
                    .params
                    .tool
                    .as_ref()
                    .map(|tool| super::bonus::item_enchantment_level(tool, enchantment))
                    .transpose()?
                    .unwrap_or(0);
                let chance = chances[(level as usize).min(chances.len() - 1)];
                Ok(c.random().next_float() < chance)
            }
            Self::RandomChanceWithEnchantedBonus {
                unenchanted,
                enchanted,
                enchantment,
            } => {
                let level = c
                    .params
                    .attacking_entity
                    .map(|entity| c.facts().entity_enchantment_level(entity, enchantment))
                    .transpose()?
                    .unwrap_or(0);
                let chance = if level > 0 {
                    enchanted.calculate(level)?
                } else {
                    *unenchanted
                };
                Ok(c.random().next_float() < chance)
            }
            Self::KilledByPlayer => Ok(c.params.last_damage_player.is_some()),
            Self::SurvivesExplosion => Ok(c
                .params
                .explosion_radius
                .is_none_or(|radius| c.random().next_float() <= 1.0 / radius)),
            Self::MatchTool(test) => {
                let Some(tool) = &c.params.tool else {
                    return Ok(false);
                };
                test.as_ref()
                    .map_or(Ok(true), |test| test.matches(tool, c.facts()))
            }
            Self::MatchBlock(test) => {
                let Some(state) = c.params.block_state else {
                    return Ok(false);
                };
                if !test.matches_state(state, c.facts())? {
                    return Ok(false);
                }
                let nbt = if let Some(expected) = &test.0.nbt {
                    let nbt = c.runtime.block_entity_nbt(c.params)?;
                    if nbt.is_none() && c.params.block_entity_components.is_some() {
                        return Err(LootError::Unsupported("loot.block_entity_nbt".into()));
                    }
                    // BlockPredicate reads NBT first and skips collectComponents
                    // when it fails. Both reads must observe the retained source.
                    if !ferraloom_nbt::compare_nbt(Some(&expected.0.0), nbt.as_ref(), true) {
                        return Ok(false);
                    }
                    nbt
                } else {
                    None
                };
                let components = if !test.0.components.exact.is_empty()
                    || !test.0.components.partial.0.is_empty()
                {
                    let components = c.runtime.block_entity_components(c.params)?;
                    if components.is_none() && c.params.block_entity.is_some() {
                        return Err(LootError::Unsupported(
                            "loot.block_entity_components".into(),
                        ));
                    }
                    components
                } else {
                    None
                };
                test.matches_entity(nbt.as_ref(), components.as_ref(), c.facts())
            }
            Self::EntityProperties(target, test) => {
                let entity = c.params.entity(*target);
                test.as_ref().map_or(Ok(entity.is_some()), |test| {
                    test.matches(entity, c.params.origin, c.facts())
                })
            }
            Self::EntityScores(target, scores) => {
                let Some(entity) = c.params.entity(*target) else {
                    return Ok(false);
                };
                for (objective, test) in scores {
                    let Some(value) = c
                        .facts()
                        .score(super::ScoreHolder::Entity(entity), objective)?
                    else {
                        return Ok(false);
                    };
                    if !test.test(value, data, c)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::LocationCheck(test, [x, y, z]) => {
                let Some(origin) = c.params.origin else {
                    return Ok(false);
                };
                test.as_ref().map_or(Ok(true), |test| {
                    test.matches(
                        Vec3::new(
                            origin.x + f64::from(*x),
                            origin.y + f64::from(*y),
                            origin.z + f64::from(*z),
                        ),
                        c.facts(),
                    )
                })
            }
            Self::WeatherCheck {
                raining: None,
                thundering: None,
            } => Ok(true),
            Self::WeatherCheck {
                raining,
                thundering,
            } => {
                let (rain, thunder) = c.facts().weather()?;
                Ok(raining.is_none_or(|v| v == rain) && thundering.is_none_or(|v| v == thunder))
            }
            Self::DamageSource(test) => {
                let Some(source) = &c.params.damage_source else {
                    return Ok(false);
                };
                let Some(origin) = c.params.origin else {
                    return Ok(false);
                };
                test.as_ref()
                    .map_or(Ok(true), |test| test.matches(source, origin, c.facts()))
            }
            Self::TimeCheck {
                clock,
                period,
                value,
            } => {
                let mut time = c.facts().clock_ticks(clock)?;
                if let Some(period) = period {
                    if *period == 0 {
                        return Err(LootError::Arithmetic);
                    }
                    time = time.wrapping_rem(*period);
                }
                value.test(time as i32, data, c)
            }
            Self::EnchantmentActive(active) => Ok(c.params.enchantment_active == Some(*active)),
            Self::IntValueCheck(input, test) => {
                if test.is_any() {
                    return Ok(true);
                }
                let value = input.get(data, c)?;
                test.test(value, data, c)
            }
            Self::FloatValueCheck(input, test) => {
                if test.is_any() {
                    return Ok(true);
                }
                let value = input.get(data, c)?;
                test.test(value, data, c)
            }
            Self::EnvironmentAttribute {
                attribute, value, ..
            } => Ok(value.same_value(&c.facts().environment_value(attribute, c.params)?)),
        }
    }
    pub(crate) fn params(&self) -> Vec<ContextParam> {
        use ContextParam::*;
        match self {
            Self::TableBonus(..) => vec![Tool],
            Self::RandomChanceWithEnchantedBonus { .. } => vec![AttackingEntity],
            Self::KilledByPlayer => vec![LastDamagePlayer],
            Self::SurvivesExplosion => vec![ExplosionRadius],
            Self::MatchTool(_) => vec![Tool],
            Self::MatchBlock(test) => {
                if test.needs_block_entity() {
                    vec![BlockState, BlockEntity]
                } else {
                    vec![BlockState]
                }
            }
            Self::EntityProperties(target, _) | Self::EntityScores(target, _) => {
                vec![target.param()]
            }
            Self::LocationCheck(..) => vec![Origin],
            Self::DamageSource(_) => vec![DamageSource, Origin],
            Self::EnchantmentActive(_) => vec![EnchantmentActive],
            Self::EnvironmentAttribute {
                positional: true, ..
            } => vec![Origin],
            _ => vec![],
        }
    }
}
impl Holder<LootItemCondition> {
    pub(crate) fn test(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<bool> {
        if let Self::Reference(id) = self {
            let key = ("predicate", id.clone());
            if !c.visited.insert(key.clone()) {
                return Ok(false);
            }
            let result = self.resolve(data)?.test(data, c);
            c.visited.remove(&key);
            result
        } else {
            self.resolve(data)?.test(data, c)
        }
    }
}
