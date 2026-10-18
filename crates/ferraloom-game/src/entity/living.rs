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
    pub(crate) fn read_additional_save_data(&mut self) {
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
