use super::{AgeableMob, Entity, EntityData, LivingEntity, Mob, PathfinderMob};
pub(crate) mod persistence;

/// Port of Animal's embedded AgeableMob parent.
#[derive(Debug)]
pub struct Animal {
    pub parent: AgeableMob,
    pub in_love: i32,
    pub love_cause: Option<super::EntityReference<super::player::ServerPlayer>>,
}

/// Port of AbstractCow, the immediate parent of Cow in vanilla 26.3.
#[derive(Debug)]
pub struct AbstractCow {
    pub parent: Animal,
}

/// Cow's canonical parent chain and bound variant metadata.
#[derive(Debug)]
pub struct Cow {
    pub parent: AbstractCow,
}

#[cfg(any(test, feature = "test-fixtures"))]
impl Default for Cow {
    fn default() -> Self {
        Self::with_parent(EntityData::default())
    }
}

impl Cow {
    #[cfg(any(test, feature = "test-fixtures"))]
    pub(crate) fn with_parent(parent: EntityData) -> Self {
        Self::with_context(parent, &crate::level::LevelContext::fixture(0, 63))
            .expect("vanilla cow fixture registries")
    }
    pub(crate) fn with_level(
        parent: EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Self::with_context(parent, level.context())
    }
    fn with_context(
        parent: EntityData,
        context: &crate::level::LevelContext,
    ) -> Result<Self, crate::level::LevelError> {
        use ferraloom_protocol::component::{CowSoundVariant, CowVariant};
        let variant =
            super::mob::default_variant(context, "minecraft:cow_variant", "minecraft:temperate")?;
        let sound = super::mob::default_variant(
            context,
            "minecraft:cow_sound_variant",
            "minecraft:classic",
        )?;
        let mob = Mob::with_parent_and_metadata(parent, context, |parent, _| {
            parent.entity_data.define(super::synched_data::BABY, false);
            parent
                .entity_data
                .define(super::synched_data::AGE_LOCKED, false);
            parent
                .entity_data
                .define(super::synched_data::COW_VARIANT, CowVariant(variant));
            parent.entity_data.define(
                super::synched_data::COW_SOUND_VARIANT,
                CowSoundVariant(sound),
            );
            Ok(())
        })?;
        Ok(Self {
            parent: AbstractCow {
                parent: Animal {
                    parent: AgeableMob {
                        parent: PathfinderMob::with_parent(mob),
                        age: 0,
                        forced_age: 0,
                    },
                    in_love: 0,
                    love_cause: None,
                },
            },
        })
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
    fn tick(
        &self,
        entity: super::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        level.entity_mut::<LivingEntity>(entity)?.swing_state.tick();
        super::base_tick(self, entity, level)?;
        // The live Mob AI/travel body remains pending; these are its inherited
        // LivingEntity damage-state slots, before the future virtual aiStep.
        super::living::damage_state::base_tick_damage_state(entity, level)?;
        if level.entity::<EntityData>(entity)?.tick_count % 20 == 0 {
            super::damage::combat_tracker::recheck_status(self, entity, level)?;
        }
        Ok(())
    }
    fn is_invulnerable_to(
        &self,
        entity: super::EntityRef,
        source: &super::DamageSource,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        super::living::hurt::living_is_invulnerable_to(self, entity, source, level)
    }
    fn hurt_sound(
        &self,
        entity: super::EntityRef,
        _source: &super::DamageSource,
        level: &crate::level::Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, crate::level::LevelError> {
        persistence::require_classic_sound(entity, level)?;
        Ok(Some(
            ferraloom_data::registries::sound_event::Id::ENTITY_COW_HURT,
        ))
    }
    fn death_sound(
        &self,
        entity: super::EntityRef,
        level: &crate::level::Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, crate::level::LevelError> {
        persistence::require_classic_sound(entity, level)?;
        Ok(Some(
            ferraloom_data::registries::sound_event::Id::ENTITY_COW_DEATH,
        ))
    }
    fn sound_volume(&self) -> f32 {
        0.4
    }
    fn weapon_item<'level>(
        &self,
        entity: super::EntityRef,
        level: &'level crate::level::Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, crate::level::LevelError>
    {
        super::living::weapon_item(self, entity, level)
    }
    fn cause_fall_damage(
        &self,
        entity: super::EntityRef,
        level: &mut crate::level::Level,
        distance: f64,
        modifier: f32,
        source: &super::DamageSource,
    ) -> Result<bool, crate::level::LevelError> {
        super::living::fall_damage::cause_fall_damage(
            self, entity, level, distance, modifier, source,
        )
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

    fn set_y_head_rot(
        &self,
        entity: super::EntityRef,
        value: f32,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        level.entity_mut::<LivingEntity>(entity)?.y_head_rot = value;
        Ok(())
    }
    fn set_y_body_rot(
        &self,
        entity: super::EntityRef,
        value: f32,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        level.entity_mut::<LivingEntity>(entity)?.y_body_rot = value;
        Ok(())
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
                persistence::baby_dimensions()
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
        state
            .as_living()
            .expect("Cow inherits LivingEntity")
            .trace_damage_entity_refs(references);
        if let Some(cow) = state.as_mob()
            && let Some(holder) = cow.leash_data.as_ref().and_then(|data| data.holder)
        {
            references.push(holder);
        }
        if let super::EntityState::Cow(cow) = state
            && let Some(cause) = cow
                .parent
                .parent
                .love_cause
                .and_then(|cause| cause.cached_entity())
        {
            references.push(cause);
        }
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
