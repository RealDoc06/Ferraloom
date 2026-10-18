use super::{DistancePredicate, LocationPredicate, NbtPredicate, PredicateContext};
use crate::{
    entity::EntityRef,
    loot::{Identifier, LootError, Result, codec},
};
use ferraloom_protocol::{codec::HolderSet, component::Persistent};
use ferraloom_util::Vec3;
use serde_json::Value;

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
    Location(LocationPredicate),
    Nbt(NbtPredicate),
    Flags(Vec<(EntityFlag, bool)>),
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
    pub fn decode(v: &Value) -> Result<Self> {
        let mut parts = Vec::new();
        for (name, value) in codec::object(v)? {
            let id = Identifier::parse(name)?;
            let part = match id.as_str() {
                "minecraft:entity_type" => EntitySubPredicate::Type(HolderSet::from_json(value)?),
                "minecraft:distance" => {
                    EntitySubPredicate::Distance(DistancePredicate::decode(value)?)
                }
                "minecraft:location" => {
                    EntitySubPredicate::Location(LocationPredicate::decode(value)?)
                }
                "minecraft:nbt" => EntitySubPredicate::Nbt(NbtPredicate::decode(value)?),
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
