use super::{AgeableMob, Entity, EntityData, LivingEntity, Mob, PathfinderMob};

/// Port of Animal's embedded AgeableMob parent.
#[derive(Debug)]
pub struct Animal {
    pub parent: AgeableMob,
    pub in_love: i32,
}

/// Port of AbstractCow, the immediate parent of Cow in vanilla 26.3.
#[derive(Debug)]
pub struct AbstractCow {
    pub parent: Animal,
}

/// Port of Cow's parent chain. Variants/synched data arrive with the cow port.
#[derive(Debug)]
pub struct Cow {
    pub parent: AbstractCow,
}

impl Default for Cow {
    fn default() -> Self {
        Self::with_parent(EntityData::default())
    }
}

impl Cow {
    pub(crate) fn with_parent(parent: EntityData) -> Self {
        Self {
            parent: AbstractCow {
                parent: Animal {
                    parent: AgeableMob {
                        parent: PathfinderMob {
                            parent: Mob {
                                parent: LivingEntity {
                                    equipment: Default::default(),
                                    parent,
                                    // AbstractCow#createAttributes, not a guessed mob default.
                                    health: 10.0,
                                    last_hurt: 0.0,
                                    damage_cooldown_time: 0,
                                    hurt_time: 0,
                                    hurt_duration: 0,
                                },
                                target: None,
                            },
                        },
                        age: 0,
                    },
                    in_love: 0,
                },
            },
        }
    }
}

impl Cow {
    pub fn as_mob(&self) -> &Mob {
        &self.parent.parent.parent.parent.parent
    }
    pub fn as_mob_mut(&mut self) -> &mut Mob {
        &mut self.parent.parent.parent.parent.parent
    }
    pub fn as_living(&self) -> &LivingEntity {
        &self.as_mob().parent
    }
    pub fn as_living_mut(&mut self) -> &mut LivingEntity {
        &mut self.as_mob_mut().parent
    }
}

/// Shared Cow dispatcher for the foundation's Entity/LivingEntity primitives.
pub struct CowBehavior;
impl Entity for CowBehavior {
    fn trace_entity_refs(
        &self,
        state: &super::EntityState,
        references: &mut Vec<super::EntityRef>,
    ) {
        super::trace_base_entity_refs(state.as_entity(), references);
        if let Some(target) = state.as_mob().and_then(|mob| mob.target) {
            references.push(target);
        }
    }

    /// Port of Mob#getPickResult for the concrete Cow type.
    fn get_pick_result(
        &self,
        _entity: super::EntityRef,
        _level: &crate::level::Level,
    ) -> Result<Option<ferraloom_protocol::component::ItemStack>, crate::level::LevelError> {
        Ok(Some(ferraloom_protocol::component::ItemStack::new(
            ferraloom_data::Item::COW_SPAWN_EGG,
            1,
            Default::default(),
        )))
    }
    fn default_gravity(&self) -> f64 {
        0.08
    }
}
