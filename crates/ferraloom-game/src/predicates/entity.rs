use super::{DistancePredicate, LocationPredicate, NbtPredicate, PredicateContext};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::{
    entity::EntityRef,
    loot::{Identifier, LootError, Result, codec},
};
use ferraloom_protocol::codec::HolderSet;
use ferraloom_util::Vec3;

/// Port of EntityFlagsPredicate. Type-specific flags await the living-entity adapter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityFlags {
    pub on_ground: bool,
    pub on_fire: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub swimming: bool,
    pub flying: bool,
    pub baby: bool,
    pub living: bool,
    pub in_water: bool,
    pub fall_flying: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum EntitySubPredicate {
    Type(HolderSet),
    Distance(DistancePredicate),
    Location(Box<LocationPredicate>),
    SteppingOn(Box<LocationPredicate>),
    MovementAffectedBy(Box<LocationPredicate>),
    Movement(super::MovementPredicate),
    Effects(ferraloom_protocol::component::MobEffectsPredicate),
    Slots(Vec<(crate::loot::SlotRange, super::ItemPredicate)>),
    Lightning {
        blocks_set_on_fire: super::IntBounds,
        entity_struck: Option<Box<EntityPredicate>>,
    },
    Player(Box<super::PlayerPredicate>),
    Nbt(NbtPredicate),
    Flags(Vec<(EntityFlag, bool)>),
    Equipment(
        Vec<(
            ferraloom_protocol::component::EquipmentSlot,
            super::ItemPredicate,
        )>,
    ),
    Team(String),
    PeriodicTick(i32),
    Vehicle(Box<EntityPredicate>),
    Passenger(Box<EntityPredicate>),
    TargetedEntity(Box<EntityPredicate>),
    Specific(SpecificPredicate),
    Tags {
        any_of: Option<Vec<String>>,
        all_of: Option<Vec<String>>,
        none_of: Option<Vec<String>>,
    },
    Components(ferraloom_protocol::component::DataComponentMatchers),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityFlag {
    OnGround,
    OnFire,
    Crouching,
    Sprinting,
    Swimming,
    Flying,
    Baby,
    InWater,
    FallFlying,
}
/// Port of EntityPredicate's registry-dispatched map, not the pre-26.3 object schema.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityPredicate {
    pub parts: Vec<EntitySubPredicate>,
}
impl EntityPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let mut parts = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        for (name, value) in codec::map(v)?.entries()? {
            let id = Identifier::parse(name)?;
            if !keys.insert(id.clone()) {
                return Err(codec::error("duplicate entity sub-predicate"));
            }
            let part = match id.as_str() {
                "minecraft:entity_type" => {
                    let set = HolderSet::from_input(value)?;
                    super::validate_static_holders(&set, "entity_type", |id| {
                        ferraloom_data::EntityType::from_name(id).is_some()
                    })?;
                    EntitySubPredicate::Type(set)
                }
                "minecraft:distance" => {
                    EntitySubPredicate::Distance(DistancePredicate::decode(value)?)
                }
                "minecraft:location" => {
                    EntitySubPredicate::Location(Box::new(LocationPredicate::decode(value)?))
                }
                "minecraft:nbt" => EntitySubPredicate::Nbt(NbtPredicate::decode(value)?),
                "minecraft:stepping_on" => {
                    EntitySubPredicate::SteppingOn(Box::new(LocationPredicate::decode(value)?))
                }
                "minecraft:movement_affected_by" => EntitySubPredicate::MovementAffectedBy(
                    Box::new(LocationPredicate::decode(value)?),
                ),
                "minecraft:movement" => {
                    EntitySubPredicate::Movement(super::MovementPredicate::decode(value)?)
                }
                "minecraft:effects" => EntitySubPredicate::Effects(
                    ferraloom_protocol::component::MobEffectsPredicate::from_input(value)?,
                ),
                "minecraft:slots" => EntitySubPredicate::Slots(
                    codec::map(value)?
                        .entries()?
                        .into_iter()
                        .map(|(key, value)| {
                            Ok((
                                crate::loot::SlotRange::decode(&serde_json::Value::String(
                                    key.to_owned(),
                                ))?,
                                super::ItemPredicate::decode(value)?,
                            ))
                        })
                        .collect::<Result<_>>()?,
                ),
                "minecraft:type_specific/player" => {
                    EntitySubPredicate::Player(Box::new(super::PlayerPredicate::decode(value)?))
                }
                "minecraft:type_specific/lightning" => EntitySubPredicate::Lightning {
                    blocks_set_on_fire: codec::optional(
                        value,
                        "blocks_set_on_fire",
                        super::IntBounds::decode,
                    )?
                    .unwrap_or_default(),
                    entity_struck: codec::optional(value, "entity_struck", |v| {
                        Ok(Box::new(Self::decode(v)?))
                    })?,
                },
                "minecraft:equipment" => {
                    use ferraloom_protocol::component::EquipmentSlot::*;
                    let mut tests = Vec::new();
                    for (name, slot) in [
                        ("head", Head),
                        ("chest", Chest),
                        ("legs", Legs),
                        ("feet", Feet),
                        ("body", Body),
                        ("mainhand", Mainhand),
                        ("offhand", Offhand),
                    ] {
                        if let Some(test) =
                            codec::optional(value, name, super::ItemPredicate::decode)?
                        {
                            tests.push((slot, test));
                        }
                    }
                    EntitySubPredicate::Equipment(tests)
                }
                "minecraft:team" => EntitySubPredicate::Team(codec::string(value)?.into()),
                "minecraft:periodic_tick" => {
                    let period = codec::int(value)?;
                    if period <= 0 {
                        return Err(codec::error("period must be positive"));
                    }
                    EntitySubPredicate::PeriodicTick(period)
                }
                "minecraft:vehicle" => EntitySubPredicate::Vehicle(Box::new(Self::decode(value)?)),
                "minecraft:passenger" => {
                    EntitySubPredicate::Passenger(Box::new(Self::decode(value)?))
                }
                "minecraft:targeted_entity" => {
                    EntitySubPredicate::TargetedEntity(Box::new(Self::decode(value)?))
                }
                "minecraft:entity_tags" => {
                    let strings = |name| {
                        codec::optional(value, name, |v| {
                            codec::list(v, |v| Ok(codec::string(v)?.to_owned()))
                        })
                    };
                    EntitySubPredicate::Tags {
                        any_of: strings("any_of")?,
                        all_of: strings("all_of")?,
                        none_of: strings("none_of")?,
                    }
                }
                "minecraft:components" => EntitySubPredicate::Components(
                    value.read_wrapped::<ferraloom_protocol::component::DataComponentMatchers>(
                        "components",
                    )?,
                ),
                "minecraft:predicates" => EntitySubPredicate::Components(
                    value.read_wrapped::<ferraloom_protocol::component::DataComponentMatchers>(
                        "predicates",
                    )?,
                ),
                "minecraft:type_specific/sheep" => EntitySubPredicate::Specific(
                    SpecificPredicate::Sheep(codec::optional(value, "sheared", codec::boolean)?),
                ),
                "minecraft:type_specific/fishing_hook" => {
                    EntitySubPredicate::Specific(SpecificPredicate::FishingHook(codec::optional(
                        value,
                        "in_open_water",
                        codec::boolean,
                    )?))
                }
                "minecraft:type_specific/cube_mob" => {
                    EntitySubPredicate::Specific(SpecificPredicate::CubeMob(
                        codec::optional(value, "size", super::IntBounds::decode)?
                            .unwrap_or_default(),
                    ))
                }
                "minecraft:type_specific/raider" => {
                    EntitySubPredicate::Specific(SpecificPredicate::Raider {
                        has_raid: codec::optional(value, "has_raid", codec::boolean)?
                            .unwrap_or(false),
                        is_captain: codec::optional(value, "is_captain", codec::boolean)?
                            .unwrap_or(false),
                    })
                }
                "minecraft:flags" => {
                    let mut flags = Vec::new();
                    for (name, flag) in [
                        ("is_on_ground", EntityFlag::OnGround),
                        ("is_on_fire", EntityFlag::OnFire),
                        ("is_sneaking", EntityFlag::Crouching),
                        ("is_sprinting", EntityFlag::Sprinting),
                        ("is_swimming", EntityFlag::Swimming),
                        ("is_flying", EntityFlag::Flying),
                        ("is_baby", EntityFlag::Baby),
                        ("is_in_water", EntityFlag::InWater),
                        ("is_fall_flying", EntityFlag::FallFlying),
                    ] {
                        if let Some(value) = codec::optional(value, name, codec::boolean)? {
                            flags.push((flag, value));
                        }
                    }
                    EntitySubPredicate::Flags(flags)
                }
                _ => return Err(LootError::Unsupported(format!("entity_predicate.{id}"))),
            };
            parts.push(part);
        }
        parts.sort_by_key(|p| match p {
            EntitySubPredicate::Type(_) => -1,
            EntitySubPredicate::Nbt(_) => 2,
            EntitySubPredicate::Vehicle(_)
            | EntitySubPredicate::Passenger(_)
            | EntitySubPredicate::TargetedEntity(_)
            | EntitySubPredicate::Player(_)
            | EntitySubPredicate::Lightning { .. } => 1,
            _ => 0,
        });
        Ok(Self { parts })
    }
    pub fn matches(
        &self,
        entity: Option<EntityRef>,
        origin: Option<Vec3>,
        facts: &dyn PredicateContext,
    ) -> Result<bool> {
        let Some(entity) = entity else {
            return Ok(false);
        };
        for part in &self.parts {
            let matches = match part {
                EntitySubPredicate::Type(set) => {
                    let kind = facts.entity_type(entity)?;
                    match set {
                        HolderSet::Direct(ids) => ids.iter().any(|v| v.as_str() == kind.name()),
                        HolderSet::Named(id) => {
                            let tags = facts.entity_tags()?;
                            let key = id.to_string().parse().map_err(
                                |e: ferraloom_data::datapack::Error| {
                                    LootError::Decode(e.to_string())
                                },
                            )?;
                            let tag = tags.resolve_name(&key).ok_or_else(|| {
                                LootError::MissingReference {
                                    registry: "entity_type tag",
                                    id: id.clone(),
                                }
                            })?;
                            tags.contains(tag, kind)
                        }
                    }
                }
                EntitySubPredicate::Distance(p) => {
                    if let Some(origin) = origin {
                        p.matches(origin, facts.entity_position(entity)?)
                    } else {
                        false
                    }
                }
                EntitySubPredicate::Location(p) => {
                    p.matches(facts.entity_position(entity)?, facts)?
                }
                EntitySubPredicate::Nbt(p) => p.matches(Some(&facts.entity_nbt(entity)?)),
                EntitySubPredicate::Movement(test) => {
                    let (movement, fall) = facts.entity_movement(entity)?;
                    test.matches(movement, fall)
                }
                EntitySubPredicate::SteppingOn(test) => {
                    if !facts.entity_flags(entity)?.on_ground {
                        false
                    } else {
                        let p = facts.entity_on_pos(entity)?;
                        test.matches(
                            Vec3::new(
                                f64::from(p.x) + 0.5,
                                f64::from(p.y) + 0.5,
                                f64::from(p.z) + 0.5,
                            ),
                            facts,
                        )?
                    }
                }
                EntitySubPredicate::MovementAffectedBy(test) => {
                    let p = facts.entity_movement_block_pos(entity)?;
                    test.matches(
                        Vec3::new(
                            f64::from(p.x) + 0.5,
                            f64::from(p.y) + 0.5,
                            f64::from(p.z) + 0.5,
                        ),
                        facts,
                    )?
                }
                EntitySubPredicate::Player(test) => test.matches(entity, facts)?,
                EntitySubPredicate::Lightning {
                    blocks_set_on_fire,
                    entity_struck,
                } => {
                    let Some((blocks, hit)) = facts.lightning_state(entity)? else {
                        return Ok(false);
                    };
                    if !blocks_set_on_fire.matches(blocks) {
                        false
                    } else if let Some(test) = entity_struck {
                        let mut found = false;
                        for hit in hit {
                            if test.matches(Some(hit), origin, facts)? {
                                found = true;
                                break;
                            }
                        }
                        found
                    } else {
                        true
                    }
                }
                EntitySubPredicate::Effects(test) => {
                    let Some(effects) = facts.entity_effects(entity)? else {
                        return Ok(false);
                    };
                    test.0.iter().all(|(kind, test)| {
                        effects
                            .iter()
                            .find(|value| value.effect == *kind)
                            .is_some_and(|value| {
                                let value = &value.details;
                                test.amplifier.min.is_none_or(|v| value.amplifier.0 >= v)
                                    && test.amplifier.max.is_none_or(|v| value.amplifier.0 <= v)
                                    && test.duration.min.is_none_or(|v| value.duration.0 >= v)
                                    && test.duration.max.is_none_or(|v| value.duration.0 <= v)
                                    && test.ambient.is_none_or(|v| v == value.ambient)
                                    && test.visible.is_none_or(|v| v == value.show_particles)
                            })
                    })
                }
                EntitySubPredicate::Slots(tests) => {
                    let mut matches = true;
                    for (range, test) in tests {
                        let mut any = false;
                        for item in facts.entity_slots(entity, &range.slots)? {
                            if test.matches(&item, facts)? {
                                any = true;
                                break;
                            }
                        }
                        if !any {
                            matches = false;
                            break;
                        }
                    }
                    matches
                }
                EntitySubPredicate::Equipment(tests) => {
                    if !facts.entity_has_equipment(entity)? {
                        return Ok(false);
                    }
                    let mut matched = true;
                    for (slot, test) in tests {
                        let stack = facts
                            .entity_equipment_slot(entity, *slot)?
                            .unwrap_or_default();
                        if !test.matches(&stack, facts)? {
                            matched = false;
                            break;
                        }
                    }
                    matched
                }
                EntitySubPredicate::Team(expected) => {
                    facts.entity_team(entity)?.as_ref() == Some(expected)
                }
                EntitySubPredicate::PeriodicTick(period) => {
                    facts.entity_tick_count(entity)? % period == 0
                }
                EntitySubPredicate::Vehicle(test) => {
                    test.matches(facts.entity_vehicle(entity)?, origin, facts)?
                }
                EntitySubPredicate::TargetedEntity(test) => {
                    test.matches(facts.entity_target(entity)?, origin, facts)?
                }
                EntitySubPredicate::Passenger(test) => {
                    let mut matched = false;
                    for passenger in facts.entity_passengers(entity)? {
                        if test.matches(Some(passenger), origin, facts)? {
                            matched = true;
                            break;
                        }
                    }
                    matched
                }
                EntitySubPredicate::Specific(test) => test.matches(entity, facts)?,
                EntitySubPredicate::Tags {
                    any_of,
                    all_of,
                    none_of,
                } => {
                    let tags = facts.entity_scoreboard_tags(entity)?;
                    any_of
                        .as_ref()
                        .is_none_or(|test| test.iter().any(|t| tags.contains(t)))
                        && none_of
                            .as_ref()
                            .is_none_or(|test| !test.iter().any(|t| tags.contains(t)))
                        && all_of
                            .as_ref()
                            .is_none_or(|test| test.iter().all(|t| tags.contains(t)))
                }
                EntitySubPredicate::Components(test) => {
                    super::match_components(test, &facts.entity_components(entity)?, facts)?
                }
                EntitySubPredicate::Flags(tests) => {
                    let flags = facts.entity_flags(entity)?;
                    tests.iter().all(|(flag, value)| {
                        matches!(flag, EntityFlag::Baby | EntityFlag::FallFlying) && !flags.living
                            || *value
                                == match flag {
                                    EntityFlag::OnGround => flags.on_ground,
                                    EntityFlag::OnFire => flags.on_fire,
                                    EntityFlag::Crouching => flags.crouching,
                                    EntityFlag::Sprinting => flags.sprinting,
                                    EntityFlag::Swimming => flags.swimming,
                                    EntityFlag::Flying => flags.flying,
                                    EntityFlag::Baby => flags.baby,
                                    EntityFlag::InWater => flags.in_water,
                                    EntityFlag::FallFlying => flags.fall_flying,
                                }
                    })
                }
            };
            if !matches {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Class-dependent facts distinguish a different class from an unavailable fact.
#[derive(Clone, Debug, PartialEq)]
pub enum EntitySpecificFacts {
    Other,
    Sheep { sheared: bool },
    FishingHook { in_open_water: bool },
    CubeMob { size: i32 },
    Raider { has_raid: bool, is_captain: bool },
}
#[derive(Clone, Debug, PartialEq)]
pub enum SpecificPredicate {
    Sheep(Option<bool>),
    FishingHook(Option<bool>),
    CubeMob(super::IntBounds),
    Raider { has_raid: bool, is_captain: bool },
}
impl SpecificPredicate {
    fn matches(&self, entity: EntityRef, facts: &dyn PredicateContext) -> Result<bool> {
        if matches!(self, Self::FishingHook(None)) {
            return Ok(true);
        }
        Ok(match (self, facts.entity_specific(entity)?) {
            (Self::Sheep(test), EntitySpecificFacts::Sheep { sheared }) => {
                test.is_none_or(|v| v == sheared)
            }
            (Self::FishingHook(test), EntitySpecificFacts::FishingHook { in_open_water }) => {
                test.is_none_or(|v| v == in_open_water)
            }
            (Self::CubeMob(test), EntitySpecificFacts::CubeMob { size }) => test.matches(size),
            (
                Self::Raider {
                    has_raid: a,
                    is_captain: b,
                },
                EntitySpecificFacts::Raider {
                    has_raid,
                    is_captain,
                },
            ) => *a == has_raid && *b == is_captain,
            _ => false,
        })
    }
}
