//! Entity's base NBT codec; subclass data and unrecognized fields remain intact.
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{chat::Component, codec::VarInt};
use ferraloom_util::{Uuid, Vec3};

use super::{EntityData, synched_data as data};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityLoadError {
    InvalidPosition,
    InvalidRotation,
}

impl EntityData {
    /// Port of Entity#load. Passengers are instantiated by the section manager.
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), EntityLoadError> {
        let position = vector(&tag, "Pos");
        let motion = vector(&tag, "Motion");
        self.velocity = Vec3::new(
            safe_motion(motion.x),
            safe_motion(motion.y),
            safe_motion(motion.z),
        );
        self.needs_sync = true;
        self.set_pos_raw(Vec3::new(
            position.x.clamp(-30_000_512.0, 30_000_512.0),
            position.y.clamp(-20_000_000.0, 20_000_000.0),
            position.z.clamp(-30_000_512.0, 30_000_512.0),
        ));
        let rotation = tag.get_list("Rotation").filter(|v| v.len() == 2);
        self.yaw = rotation.and_then(|v| v[0].as_float()).unwrap_or(0.0);
        self.pitch = rotation.and_then(|v| v[1].as_float()).unwrap_or(0.0);
        self.set_old_pos_and_rot();
        self.fall_distance = tag.get_double_or("fall_distance", 0.0);
        self.remaining_fire_ticks = i32::from(tag.get_short_or("Fire", 0));
        self.entity_data
            .set(data::AIR_SUPPLY, VarInt(tag.get_int_or("Air", 300)));
        self.on_ground = tag.get_boolean_or("OnGround", false);
        self.permanently_invulnerable = tag.get_boolean_or("Invulnerable", false);
        self.invulnerable_time = tag.get_int_or("invulnerable_time", 0);
        self.portal_cooldown = tag.get_int_or("PortalCooldown", 0);
        if let Some(words) = tag.get_int_array("UUID")
            && let Ok(words) = <[i32; 4]>::try_from(words)
        {
            self.uuid = Uuid::from_int_array(words);
        }
        if !self.position.is_finite() {
            return Err(EntityLoadError::InvalidPosition);
        }
        if !self.yaw.is_finite() || !self.pitch.is_finite() {
            return Err(EntityLoadError::InvalidRotation);
        }
        self.set_pos(self.position);
        self.yaw %= 360.0;
        self.pitch %= 360.0;
        let name = tag
            .get("CustomName")
            .cloned()
            .and_then(|value| Component::from_nbt(value).ok())
            .map(Box::new);
        self.entity_data.set(data::CUSTOM_NAME, name);
        self.entity_data.set(
            data::CUSTOM_NAME_VISIBLE,
            tag.get_boolean_or("CustomNameVisible", false),
        );
        self.entity_data
            .set(data::SILENT, tag.get_boolean_or("Silent", false));
        self.entity_data
            .set(data::NO_GRAVITY, tag.get_boolean_or("NoGravity", false));
        self.has_glowing_tag = tag.get_boolean_or("Glowing", false);
        self.entity_data
            .set(data::TICKS_FROZEN, VarInt(tag.get_int_or("TicksFrozen", 0)));
        self.has_visual_fire = tag.get_boolean_or("HasVisualFire", false);
        self.custom_data = tag.get_compound_or_empty("data");
        self.tags.clear();
        if let Some(tags) = tag.get_list("Tags")
            && tags.len() <= 1024
            && tags.iter().all(|v| matches!(v, Tag::String(_)))
        {
            for value in tags.iter() {
                self.tags
                    .insert(value.as_string().unwrap().to_string_lossy());
            }
        }
        self.saved = tag;
        Ok(())
    }
    /// Port of Entity#saveWithoutId. The Level facade supplies vehicle X/Z and riders.
    pub fn save_without_id(&self) -> CompoundTag {
        let mut tag = self.saved.clone();
        for key in [
            "id",
            "invulnerable_time",
            "CustomName",
            "CustomNameVisible",
            "Silent",
            "NoGravity",
            "Glowing",
            "TicksFrozen",
            "HasVisualFire",
            "Tags",
            "data",
            "Passengers",
        ] {
            tag.remove(key);
        }
        tag.insert("Pos", vector_tag(self.position));
        tag.insert("Motion", vector_tag(self.velocity));
        tag.insert(
            "Rotation",
            Tag::List(ListTag(vec![Tag::Float(self.yaw), Tag::Float(self.pitch)])),
        );
        tag.insert("fall_distance", Tag::Double(self.fall_distance));
        tag.insert("Fire", Tag::Short(self.remaining_fire_ticks as i16));
        tag.insert(
            "Air",
            Tag::Short(self.entity_data.get(data::AIR_SUPPLY).0 as i16),
        );
        tag.insert("OnGround", Tag::Byte(i8::from(self.on_ground)));
        tag.insert(
            "Invulnerable",
            Tag::Byte(i8::from(self.permanently_invulnerable)),
        );
        tag.insert("PortalCooldown", Tag::Int(self.portal_cooldown));
        tag.insert("UUID", Tag::IntArray(self.uuid.to_int_array().to_vec()));
        if self.invulnerable_time > 0 {
            tag.insert("invulnerable_time", Tag::Int(self.invulnerable_time));
        }
        if let Some(name) = self.entity_data.get(data::CUSTOM_NAME) {
            tag.insert("CustomName", name.to_nbt().expect("valid component"));
        }
        for (key, value) in [
            (
                "CustomNameVisible",
                *self.entity_data.get(data::CUSTOM_NAME_VISIBLE),
            ),
            ("Silent", *self.entity_data.get(data::SILENT)),
            ("NoGravity", *self.entity_data.get(data::NO_GRAVITY)),
            ("Glowing", self.has_glowing_tag),
            ("HasVisualFire", self.has_visual_fire),
        ] {
            if value {
                tag.insert(key, Tag::Byte(1));
            }
        }
        if self.ticks_frozen() > 0 {
            tag.insert("TicksFrozen", Tag::Int(self.ticks_frozen()));
        }
        if !self.tags.is_empty() {
            tag.insert(
                "Tags",
                Tag::List(ListTag(
                    self.tags
                        .iter()
                        .map(|v| Tag::String(v.as_str().into()))
                        .collect(),
                )),
            );
        }
        if !self.custom_data.is_empty() {
            tag.insert("data", Tag::Compound(self.custom_data.clone()));
        }
        tag
    }
}
fn safe_motion(value: f64) -> f64 {
    if value.abs() > 10.0 { 0.0 } else { value }
}
fn vector(tag: &CompoundTag, key: &str) -> Vec3 {
    let Some(list) = tag.get_list(key).filter(|v| v.len() == 3) else {
        return Vec3::ZERO;
    };
    match (
        list[0].as_double(),
        list[1].as_double(),
        list[2].as_double(),
    ) {
        (Some(x), Some(y), Some(z)) => Vec3::new(x, y, z),
        _ => Vec3::ZERO,
    }
}
pub(super) fn vector_tag(value: Vec3) -> Tag {
    Tag::List(ListTag(vec![
        Tag::Double(value.x),
        Tag::Double(value.y),
        Tag::Double(value.z),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::EntityType;
    #[test]
    fn base_nbt_round_trips_and_removes_cleared_optional_fields() {
        let mut base = EntityData::new(EntityType::ITEM, 9, 123);
        base.set_pos(Vec3::new(-0.0, 80.25, 17.0));
        base.fall_distance = 42.123456789;
        base.remaining_fire_ticks = -20;
        base.invulnerable_time = 17;
        base.entity_data.set(data::TICKS_FROZEN, VarInt(120));
        base.entity_data.set(
            data::CUSTOM_NAME,
            Some(Box::new(Component::literal("named"))),
        );
        base.custom_data.insert("future", Tag::Long(123));
        base.saved.insert("FutureField", Tag::Int(42));
        base.tags.insert("one".into());
        base.tags.insert("two".into());
        let tag = base.save_without_id();
        let mut loaded = EntityData::new(EntityType::ITEM, 10, 0);
        loaded.load_nbt(tag.clone()).unwrap();
        assert_eq!(loaded.save_without_id(), tag);
        assert_eq!(loaded.fall_distance.to_bits(), base.fall_distance.to_bits());
        loaded.entity_data.set(data::CUSTOM_NAME, None);
        loaded.entity_data.set(data::TICKS_FROZEN, VarInt(0));
        loaded.invulnerable_time = 0;
        let tag = loaded.save_without_id();
        assert!(!tag.contains_key("CustomName"));
        assert!(!tag.contains_key("TicksFrozen"));
        assert!(!tag.contains_key("invulnerable_time"));
        assert_eq!(tag.get_int("FutureField"), Some(42));
    }
    #[test]
    fn load_clamps_coordinates_and_excessive_velocity() {
        let mut tag = CompoundTag::new();
        tag.insert(
            "Pos",
            vector_tag(Vec3::new(f64::INFINITY, -30_000_000.0, -40_000_000.0)),
        );
        tag.insert("Motion", vector_tag(Vec3::new(10.0, -10.00001, 0.25)));
        let mut base = EntityData::default();
        base.load_nbt(tag).unwrap();
        assert_eq!(
            base.position,
            Vec3::new(30_000_512.0, -20_000_000.0, -30_000_512.0)
        );
        assert_eq!(base.velocity, Vec3::new(10.0, 0.0, 0.25));
        let mut tag = CompoundTag::new();
        tag.insert("Pos", vector_tag(Vec3::new(f64::NAN, 0.0, 0.0)));
        assert_eq!(base.load_nbt(tag), Err(EntityLoadError::InvalidPosition));
    }
}

/// Port of BlockState.CODEC: default states use the registry string shorthand.
pub(crate) fn save_block_state(state: ferraloom_data::BlockState) -> Tag {
    if state == state.block().default_state() {
        return Tag::String(state.block().name().into());
    }
    let mut tag = CompoundTag::new();
    tag.insert("id", Tag::String(state.block().name().into()));
    let mut properties = CompoundTag::new();
    for (name, value) in state.properties() {
        properties.insert(name, Tag::String(value.into()));
    }
    tag.insert("properties", Tag::Compound(properties));
    Tag::Compound(tag)
}
pub(crate) fn load_block_state(tag: &Tag) -> Option<ferraloom_data::BlockState> {
    use ferraloom_data::{Block, BlockState};
    if let Some(name) = tag.as_string() {
        return Block::from_name(&name.to_string_lossy()).map(Block::default_state);
    }
    let tag = tag.as_compound()?;
    let block = Block::from_name(&tag.get_string("id")?.to_string_lossy())?;
    let properties: Vec<_> = tag
        .get_compound("properties")
        .map(|props| {
            props
                .iter()
                .filter_map(|(key, value)| {
                    Some((key.to_string_lossy(), value.as_string()?.to_string_lossy()))
                })
                .collect()
        })
        .unwrap_or_default();
    BlockState::from_properties(
        block,
        properties
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    )
}
