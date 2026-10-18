use super::{
    EntityData, ai::attributes::AttributeMap, player::equipment::EntityEquipment,
    synched_data as data,
};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::codec::VarInt;
use ferraloom_util::random::RandomSource;

/// Port of LivingEntity's canonical attributes, metadata, equipment and current
/// unarmored damage primitive. Every concrete subclass embeds this state once.
#[derive(Debug)]
pub struct LivingEntity {
    pub parent: EntityData,
    pub attributes: AttributeMap,
    pub equipment: EntityEquipment,
    pub y_head_rot: f32,
    pub old_y_head_rot: f32,
    pub y_body_rot: f32,
    pub old_y_body_rot: f32,
    pub last_hurt: f32,
    pub damage_cooldown_time: i32,
    pub hurt_time: i32,
    pub hurt_duration: i32,
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
            y_head_rot: 0.0,
            old_y_head_rot: 0.0,
            y_body_rot: 0.0,
            old_y_body_rot: 0.0,
            last_hurt: 0.0,
            damage_cooldown_time: 0,
            hurt_time: 0,
            hurt_duration: 0,
        };
        living.set_health(living.max_health());
        living.parent.blocks_building = true;
        living.parent.set_pos(living.parent.position);
        let rotation = living.parent.random.next_float() * std::f32::consts::TAU;
        living.parent.set_y_rot(rotation);
        living.y_head_rot = living.parent.yaw;
        living
    }
    pub fn health(&self) -> f32 {
        *self.parent.entity_data.get(data::HEALTH)
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
        let rotation = self.parent.load_nbt_with_head_rotation(tag)?;
        self.y_head_rot = rotation;
        self.y_body_rot = rotation;
        self.read_additional_save_data();
        Ok(())
    }
    fn read_additional_save_data(&mut self) {
        if let Some(attributes) = self.parent.saved.get("attributes") {
            for error in self.attributes.load_nbt(attributes) {
                eprintln!(
                    "Failed to load attributes for entity {}: {error:?}",
                    self.parent.uuid
                );
            }
        }
        self.set_health(self.parent.saved.get_float_or("Health", self.max_health()));
        self.hurt_time = i32::from(self.parent.saved.get_short_or("HurtTime", 0));
        self.equipment = self
            .parent
            .saved
            .get("equipment")
            .and_then(|tag| EntityEquipment::from_tag(tag).ok())
            .unwrap_or_default();
    }
    pub(crate) fn add_additional_save_data(&self, tag: &mut CompoundTag) {
        tag.insert("Health", Tag::Float(self.health()));
        tag.insert("HurtTime", Tag::Short(self.hurt_time as i16));
        tag.insert("attributes", self.attributes.save_nbt());
        tag.remove("equipment");
        if !self.equipment.is_empty() {
            tag.insert(
                "equipment",
                Tag::Compound(self.equipment.save().expect("canonical equipment")),
            );
        }
    }
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
    Ok((1.0_f32 - (1.0_f32 - 0.98_f32) * modifier).clamp(0.0, 1.0))
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
