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
                                parent: {
                                    let mut living = LivingEntity::with_parent(parent);
                                    living.entity_data.define(super::synched_data::MOB_FLAGS, 0);
                                    living.entity_data.define(super::synched_data::BABY, false);
                                    living
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
    fn weapon_item<'level>(
        &self,
        entity: super::EntityRef,
        level: &'level crate::level::Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, crate::level::LevelError>
    {
        super::living::weapon_item(self, entity, level)
    }
    fn check_fall_damage(
        &self,
        entity: super::EntityRef,
        dy: f64,
        on_ground: bool,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        super::living::fall_damage::check_fall_damage(
            self, entity, dy, on_ground, state, pos, level,
        )
    }
    fn on_changed_block(
        &self,
        entity: super::EntityRef,
        _pos: ferraloom_util::BlockPos,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        super::living::fall_damage::on_changed_block(entity, level)
    }
    fn jump_in_liquid(
        &self,
        _entity: super::EntityRef,
        _level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        // Mob#getNavigation().canFloat() needs the canonical navigation state.
        Err(crate::level::LevelError::UnsupportedEntityMethod(
            ferraloom_data::EntityType::COW,
            super::EntityMethod::JumpInLiquid,
        ))
    }
    /// Port of AgeableMob#isBaby; AbstractCow permits baby ages.
    fn is_baby(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        Ok(level.entity::<Cow>(entity)?.parent.parent.parent.age < 0)
    }
    fn can_freeze(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        super::living::freezing::can_freeze(self, entity, level)
    }
    fn is_alive(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        let cow = level.entity::<Cow>(entity)?;
        Ok(!cow.as_living().removed && cow.as_living().health() > 0.0)
    }
    fn push(
        &self,
        entity: crate::entity::EntityRef,
        other: crate::entity::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        crate::entity::pushing::living_push(self, entity, other, level)
    }
    fn is_pushable(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        crate::entity::living::climbable::is_pushable(self, entity, level)
    }
    fn controlling_passenger(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<Option<super::EntityRef>, crate::level::LevelError> {
        let cow = level.entity::<Cow>(entity)?;
        if *cow
            .as_living()
            .entity_data
            .get(super::synched_data::MOB_FLAGS) as u8
            & 1
            != 0
        {
            return Ok(None);
        }
        let Some(&rider) = cow.as_living().passengers.first() else {
            return Ok(None);
        };
        let Some(mob) = level.entity_state(rider)?.as_mob() else {
            return Ok(None);
        };
        Ok((!level.entity_type_tags()?.contains_key(
            ferraloom_data::tags::EntityTypeTags::NON_CONTROLLING_RIDER,
            mob.parent.entity_type,
        ))
        .then_some(rider))
    }

    fn y_head_rot(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        Ok(level.entity::<Cow>(entity)?.as_living().y_head_rot)
    }
    fn dimensions(
        &self,
        entity: super::EntityRef,
        pose: ferraloom_protocol::entity_data::Pose,
        level: &crate::level::Level,
    ) -> Result<super::EntityDimensions, crate::level::LevelError> {
        super::living::dimensions(self, entity, pose, level)
    }
    fn default_dimensions(
        &self,
        entity: super::EntityRef,
        _pose: ferraloom_protocol::entity_data::Pose,
        level: &crate::level::Level,
    ) -> Result<super::EntityDimensions, crate::level::LevelError> {
        let dimensions = super::EntityDimensions::for_type(ferraloom_data::EntityType::COW);
        Ok(
            if level.entity::<Cow>(entity)?.parent.parent.parent.age < 0 {
                dimensions.scale(0.5, 0.5).with_eye_height(0.665)
            } else {
                dimensions
            },
        )
    }
    fn passenger_riding_position(
        &self,
        entity: super::EntityRef,
        passenger: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<ferraloom_util::Vec3, crate::level::LevelError> {
        super::living::passenger_riding_position(self, entity, passenger, level)
    }
    fn ride_tick(
        &self,
        entity: super::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        super::living_ride_tick(self, entity, level)
    }

    fn ride_tick_with_tick_rate(
        &self,
        entity: super::EntityRef,
        level: &mut crate::level::Level,
        tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), crate::level::LevelError> {
        super::living_ride_tick_with_tick_rate(self, entity, level, tick_rate)
    }

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
    fn default_gravity(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::GRAVITY,
            level,
        )
    }
    fn effective_gravity(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        super::living::travel::effective_gravity(self, entity, level)
    }
    fn max_up_step(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        Ok(crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::STEP_HEIGHT,
            level,
        )? as f32)
    }
    fn bounciness(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::BOUNCINESS,
            level,
        )
    }
    fn air_drag(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        crate::entity::living::air_drag(entity, level)
    }
    fn block_speed_factor(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        crate::entity::living::block_speed_factor(entity, level)
    }
    fn ignite_for_ticks(
        &self,
        entity: crate::entity::EntityRef,
        ticks: i32,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        crate::entity::living::ignite_for_ticks(entity, ticks, level)
    }
}
