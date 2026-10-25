use super::{
    EntityData, ai::attributes::AttributeMap, player::equipment::EntityEquipment,
    synched_data as data,
};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{codec::VarInt, component::ItemStack};
use ferraloom_util::random::RandomSource;

pub mod climbable;
pub mod damage_state;
mod equipment;
pub use equipment::on_equipped_item_broken;
pub(crate) use equipment::on_equipped_item_broken_parent;
pub mod fall_damage;
pub mod freezing;
pub mod gliding;
pub mod jump;
pub mod travel;

/// Port of LivingEntity.Fallsounds, selected virtually by the entity family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FallSounds {
    pub small: ferraloom_data::registries::sound_event::Id,
    pub big: ferraloom_data::registries::sound_event::Id,
}

/// Port of LivingEntity's canonical attributes, metadata, equipment and current
/// unarmored damage primitive. Every concrete subclass embeds this state once.
#[derive(Debug)]
pub struct LivingEntity {
    pub parent: EntityData,
    pub attributes: AttributeMap,
    pub equipment: EntityEquipment,
    pub(crate) active_effects: super::effect::ActiveEffects,
    pub effects_dirty: bool,
    pub use_item: ItemStack,
    pub use_item_remaining: i32,
    pub jumping: bool,
    pub xxa: f32,
    pub yya: f32,
    pub zza: f32,
    pub no_jump_delay: i32,
    pub fall_fly_ticks: i32,
    /// Only startAutoSpinAttack (unported riptide) can make this positive.
    pub auto_spin_attack_ticks: i32,
    pub speed: f32,
    pub discard_friction: bool,
    pub last_climbable_pos: std::cell::Cell<Option<ferraloom_util::BlockPos>>,
    pub y_head_rot: f32,
    pub old_y_head_rot: f32,
    pub y_body_rot: f32,
    pub old_y_body_rot: f32,
    pub last_hurt: f32,
    pub damage_cooldown_time: i32,
    pub hurt_time: i32,
    pub hurt_duration: i32,
    pub death_time: i32,
    pub(super) combat_tracker: super::damage::combat_tracker::CombatTracker,
    pub(super) no_action_time: i32,
    pub(super) absorption_amount: f32,
    pub(super) last_hurt_by_player: Option<super::EntityReference<super::player::ServerPlayer>>,
    pub(super) last_hurt_by_player_memory_time: i32,
    pub(super) last_hurt_by_mob: Option<super::EntityReference<LivingEntity>>,
    pub(super) last_hurt_by_mob_timestamp: i32,
    pub(super) last_damage_source: Option<super::DamageSource>,
    pub(super) last_damage_stamp: i64,
}
impl LivingEntity {
    /// Port of LivingEntity's constructor after Entity's derived metadata build.
    pub(crate) fn with_parent(mut parent: EntityData) -> Self {
        let entity_data = &mut parent.entity_data;
        entity_data.define(data::LIVING_ENTITY_FLAGS, 0);
        entity_data.define(data::HEALTH, 1.0);
        entity_data.define(data::EFFECT_PARTICLES, Vec::new());
        entity_data.define(data::EFFECT_AMBIENCE, false);
        entity_data.define(data::ARROW_COUNT, VarInt(0));
        entity_data.define(data::STINGER_COUNT, VarInt(0));
        entity_data.define(data::SLEEPING_POS, None);
        let attributes = AttributeMap::new(parent.entity_type);
        let mut living = Self {
            parent,
            attributes,
            equipment: EntityEquipment::default(),
            active_effects: super::effect::ActiveEffects::default(),
            effects_dirty: true,
            use_item: ItemStack::default(),
            use_item_remaining: 0,
            jumping: false,
            xxa: 0.0,
            yya: 0.0,
            zza: 0.0,
            no_jump_delay: 0,
            fall_fly_ticks: 0,
            auto_spin_attack_ticks: 0,
            speed: 0.0,
            discard_friction: false,
            last_climbable_pos: std::cell::Cell::new(None),
            y_head_rot: 0.0,
            old_y_head_rot: 0.0,
            y_body_rot: 0.0,
            old_y_body_rot: 0.0,
            last_hurt: 0.0,
            damage_cooldown_time: 0,
            hurt_time: 0,
            hurt_duration: 0,
            death_time: 0,
            combat_tracker: Default::default(),
            no_action_time: 0,
            absorption_amount: 0.0,
            last_hurt_by_player: None,
            last_hurt_by_player_memory_time: 0,
            last_hurt_by_mob: None,
            last_hurt_by_mob_timestamp: 0,
            last_damage_source: None,
            last_damage_stamp: 0,
        };
        living.set_health(living.max_health());
        living.parent.blocks_building = true;
        living.parent.set_pos(living.parent.position);
        let rotation = living.parent.random.next_float() * std::f32::consts::TAU;
        living.parent.set_y_rot(rotation);
        living.y_head_rot = living.parent.yaw;
        living
    }
    /// Port of LivingEntity#isUsingItem (mask 1, not a bit index).
    pub fn is_using_item(&self) -> bool {
        self.parent.entity_data.get(data::LIVING_ENTITY_FLAGS) & 1 > 0
    }
    /// Port of LivingEntity#getUseItem.
    pub fn get_use_item(&self) -> &ItemStack {
        &self.use_item
    }
    /// Port of LivingEntity#getUseItemRemainingTicks.
    pub fn get_use_item_remaining_ticks(&self) -> i32 {
        self.use_item_remaining
    }
    /// Port of LivingEntity#setLivingEntityFlag, preserving the other byte flags.
    pub fn set_living_entity_flag(&mut self, mask: i32, value: bool) {
        let current = i32::from(*self.parent.entity_data.get(data::LIVING_ENTITY_FLAGS));
        let flags = if value {
            current | mask
        } else {
            current & !mask
        };
        self.parent
            .entity_data
            .set(data::LIVING_ENTITY_FLAGS, flags as i8);
    }
    pub fn health(&self) -> f32 {
        *self.parent.entity_data.get(data::HEALTH)
    }
    /// Port of LivingEntity#getEffect over the canonical holder-keyed map.
    pub fn get_effect(
        &self,
        effect: ferraloom_data::registries::mob_effect::Id,
    ) -> Option<&super::effect::MobEffectInstance> {
        self.active_effects.get(effect)
    }
    pub fn has_effect(&self, effect: ferraloom_data::registries::mob_effect::Id) -> bool {
        self.active_effects.contains(effect)
    }
    pub fn max_health(&self) -> f32 {
        self.attributes
            .value(Attribute::MAX_HEALTH)
            .expect("living entity supplier defines MAX_HEALTH") as f32
    }
    /// Port of LivingEntity#setHealth, including Java's NaN and signed-zero behavior.
    pub fn set_health(&mut self, health: f32) {
        self.parent
            .entity_data
            .set(data::HEALTH, health.clamp(0.0, self.max_health()));
    }
    /// Port of Entity#load's LivingEntity head/body setters followed by the
    /// inherited LivingEntity save-data body.
    pub(crate) fn load_nbt(
        &mut self,
        tag: CompoundTag,
    ) -> Result<(), super::persistence::EntityLoadError> {
        self.load_nbt_with_absorption_setter(tag, LivingEntity::internal_set_absorption_amount)
    }
    /// Cold loading retains the derived absorption setter before attributes,
    /// without installing a partially loaded entity in Level.
    pub(crate) fn load_nbt_with_absorption_setter(
        &mut self,
        tag: CompoundTag,
        set_absorption: fn(&mut LivingEntity, f32),
    ) -> Result<(), super::persistence::EntityLoadError> {
        let rotation = self.parent.load_nbt_with_head_rotation(tag)?;
        self.y_head_rot = rotation;
        self.y_body_rot = rotation;
        self.read_additional_save_data(set_absorption);
        Ok(())
    }
    fn read_additional_save_data(&mut self, set_absorption: fn(&mut LivingEntity, f32)) {
        set_absorption(
            self,
            self.parent.saved.get_float_or("AbsorptionAmount", 0.0),
        );
        if let Some(attributes) = self.parent.saved.get("attributes") {
            for error in self.attributes.load_nbt(attributes) {
                eprintln!(
                    "Failed to load attributes for entity {}: {error:?}",
                    self.parent.uuid
                );
            }
        }
        for error in self
            .active_effects
            .load(self.parent.saved.get("active_effects"))
        {
            eprintln!(
                "Failed to load active effects for entity {}: {error}",
                self.parent.uuid
            );
        }
        if !self.active_effects.is_empty() {
            self.effects_dirty = true;
        }
        self.set_health(self.parent.saved.get_float_or("Health", self.max_health()));
        self.hurt_time = i32::from(self.parent.saved.get_short_or("HurtTime", 0));
        self.death_time = i32::from(self.parent.saved.get_short_or("DeathTime", 0));
        self.parent
            .set_shared_flag(7, self.parent.saved.get_boolean_or("FallFlying", false));
        self.last_hurt_by_player =
            damage_state::read_reference(&self.parent.saved, "last_hurt_by_player");
        self.last_hurt_by_player_memory_time = self
            .parent
            .saved
            .get_int_or("last_hurt_by_player_memory_time", 0);
        self.last_hurt_by_mob =
            damage_state::read_reference(&self.parent.saved, "last_hurt_by_mob");
        self.last_hurt_by_mob_timestamp = self.parent.tick_count.wrapping_sub(
            self.parent
                .saved
                .get_int_or("ticks_since_last_hurt_by_mob", 0),
        );
        self.equipment = self
            .parent
            .saved
            .get("equipment")
            .and_then(|tag| EntityEquipment::from_tag(tag).ok())
            .unwrap_or_default();
    }
    pub(crate) fn add_additional_save_data(
        &self,
        tag: &mut CompoundTag,
        level: &crate::level::Level,
        get_absorption: fn(&LivingEntity) -> f32,
    ) -> Result<(), crate::level::LevelError> {
        tag.insert("Health", Tag::Float(self.health()));
        tag.insert("HurtTime", Tag::Short(self.hurt_time as i16));
        tag.insert("DeathTime", Tag::Short(self.death_time as i16));
        let absorption = get_absorption(self);
        // TagValueOutput#putFloat uses FloatTag.valueOf's shared positive zero.
        tag.insert(
            "AbsorptionAmount",
            Tag::Float(if absorption == 0.0 { 0.0 } else { absorption }),
        );
        tag.insert("attributes", self.attributes.save_nbt());
        tag.remove("active_effects");
        if !self.active_effects.is_empty() {
            // Deterministic identity-map order, not native holder hash iteration.
            tag.insert("active_effects", self.active_effects.save());
        }
        tag.insert(
            "FallFlying",
            Tag::Byte(i8::from(self.parent.shared_flag(7))),
        );
        damage_state::save_references(self, tag, level)?;
        tag.remove("equipment");
        if !self.equipment.is_empty() {
            tag.insert(
                "equipment",
                Tag::Compound(
                    self.equipment.save().map_err(|_| {
                        crate::level::LevelError::Inventory("invalid saved equipment")
                    })?,
                ),
            );
        }
        Ok(())
    }
}

/// Port of server-side LivingEntity#stopUsingItem and ItemStack#causeUseVibration.
/// Clear the flag before the game event, then reset the stack and remaining ticks.
pub fn stop_using_item(
    entity: super::EntityRef,
    level: &mut crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    let (position, vibrates) = {
        use ferraloom_data::registries::data_component_type::Id as ComponentType;
        use ferraloom_protocol::component::DataComponent;
        let state = level.entity_mut::<LivingEntity>(entity)?;
        let was_using = state.is_using_item();
        state.set_living_entity_flag(1, false);
        let vibrates = was_using
            && !state.use_item.is_empty()
            && matches!(
                super::player::inventory::component(&state.use_item, ComponentType::USE_EFFECTS),
                Some(DataComponent::UseEffects(value)) if value.interact_vibrations
            );
        (state.position, vibrates)
    };
    if vibrates {
        level.game_event(
            ferraloom_data::registries::game_event::Id::ITEM_INTERACT_FINISH,
            position,
            Some(entity),
            None,
        )?;
    }
    let state = level.entity_mut::<LivingEntity>(entity)?;
    state.use_item = ItemStack::default();
    state.use_item_remaining = 0;
    Ok(())
}

/// Embedded-parent projection. Each struct dereferences to its immediate parent.
impl std::ops::Deref for LivingEntity {
    type Target = EntityData;
    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}
impl std::ops::DerefMut for LivingEntity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}

/// Port of LivingEntity#getItemBySlot and PlayerEquipment's main-hand override.
/// The selected inventory stack is canonical for players; no equipment copy is kept.
pub fn item_by_slot(
    entity: super::EntityRef,
    slot: ferraloom_protocol::component::EquipmentSlot,
    level: &crate::level::Level,
) -> Result<ferraloom_protocol::component::ItemStack, crate::level::LevelError> {
    let state = level.entity_state(entity)?;
    if let super::EntityState::Player(player) = state
        && slot == ferraloom_protocol::component::EquipmentSlot::Mainhand
    {
        return Ok(player.inventory.selected_item().clone());
    }
    Ok(state
        .as_living()
        .ok_or(crate::level::LevelError::WrongEntityType(entity))?
        .equipment
        .get(slot)
        .clone())
}

/// Port of LivingEntity#getWeaponItem, retaining the derived main-hand accessor.
pub fn weapon_item<'level, E: super::Entity + ?Sized>(
    behavior: &E,
    entity: super::EntityRef,
    level: &'level crate::level::Level,
) -> Result<Option<&'level ItemStack>, crate::level::LevelError> {
    behavior.item_by_slot(
        entity,
        ferraloom_protocol::component::EquipmentSlot::Mainhand,
        level,
    )
}

/// Port of LivingEntity#getAttributeValue for its required supplier attributes.
pub fn attribute(
    entity: super::EntityRef,
    attribute: Attribute,
    level: &crate::level::Level,
) -> Result<f64, crate::level::LevelError> {
    Ok(level
        .entity::<LivingEntity>(entity)?
        .attributes
        .value(attribute)
        .expect("living supplier defines this required attribute"))
}
/// Port of LivingEntity#getBlockSpeedFactor. Capture efficiency before calling
/// the Entity parent, then interpolate using native float arithmetic.
pub fn block_speed_factor(
    entity: super::EntityRef,
    level: &crate::level::Level,
) -> Result<f32, crate::level::LevelError> {
    let efficiency = attribute(entity, Attribute::MOVEMENT_EFFICIENCY, level)? as f32;
    let base = super::movement::block_speed_factor(entity, level)?;
    Ok(base + efficiency * (1.0 - base))
}
/// Port of LivingEntity#getAirDrag for the supported ground-moving families.
pub fn air_drag(
    entity: super::EntityRef,
    level: &crate::level::Level,
) -> Result<f32, crate::level::LevelError> {
    let modifier = attribute(entity, Attribute::AIR_DRAG_MODIFIER, level)? as f32;
    Ok(travel::compute_modified_friction(0.98, modifier))
}
/// Port of LivingEntity#igniteForTicks followed by the Entity parent body.
pub fn ignite_for_ticks(
    entity: super::EntityRef,
    ticks: i32,
    level: &mut crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    let scaled = f64::from(ticks) * attribute(entity, Attribute::BURNING_TIME, level)?;
    let ticks = ferraloom_util::mth::ceil(scaled);
    level
        .entity_mut::<EntityData>(entity)?
        .ignite_for_ticks(ticks);
    Ok(())
}

/// Port of LivingEntity#getDimensions, retaining the subclass's default pose
/// dimensions and applying the live SCALE attribute as a separate float step.
pub fn dimensions<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: super::EntityRef,
    pose: ferraloom_protocol::entity_data::Pose,
    level: &crate::level::Level,
) -> Result<super::EntityDimensions, crate::level::LevelError> {
    if pose == ferraloom_protocol::entity_data::Pose::Sleeping {
        return Ok(super::EntityDimensions::fixed(0.2, 0.2).with_eye_height(0.2));
    }
    let dimensions = behavior.default_dimensions(entity, pose, level)?;
    let scale = level
        .entity::<LivingEntity>(entity)?
        .attributes
        .value(Attribute::SCALE)
        .expect("living supplier defines SCALE") as f32;
    Ok(dimensions.scale(scale, scale))
}

/// Port of LivingEntity#getPassengerRidingPosition's current dimension query.
pub fn passenger_riding_position<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: super::EntityRef,
    passenger: super::EntityRef,
    level: &crate::level::Level,
) -> Result<ferraloom_util::Vec3, crate::level::LevelError> {
    let (position, pose, scale, age_scale) = {
        let state = level.entity_state(entity)?;
        let base = state.as_entity();
        let living = state
            .as_living()
            .ok_or(crate::level::LevelError::WrongEntityType(entity))?;
        let scale = living
            .attributes
            .value(Attribute::SCALE)
            .expect("living supplier defines SCALE") as f32;
        let baby =
            matches!(state, super::EntityState::Cow(cow) if cow.parent.parent.parent.age < 0);
        (
            base.position,
            *base.entity_data.get(data::POSE),
            scale,
            if baby { 0.5 } else { 1.0 },
        )
    };
    let dimensions = behavior.dimensions(entity, pose, level)?;
    Ok(position
        + behavior.passenger_attachment_point(
            entity,
            passenger,
            &dimensions,
            scale * age_scale,
            level,
        )?)
}

#[cfg(test)]
mod tests;
