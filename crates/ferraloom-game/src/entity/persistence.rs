//! Entity's base NBT codec; subclass data and unrecognized fields remain intact.
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{chat::Component, codec::VarInt, component::Persistent};
use ferraloom_util::{Uuid, Vec3};

use super::{EntityData, synched_data as data};

pub(crate) mod live;
mod recursive;
mod tree;
pub use live::load_in_place;
pub use recursive::load_entity_recursive;
pub use tree::EntityPersistenceError;
pub(crate) use tree::load_entity_tree;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityLoadError {
    InvalidPosition,
    InvalidRotation,
    RecipeManagerRequired,
}
impl std::fmt::Display for EntityLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EntityLoadError {}

/// The supported constructor set is shared by persistent and proto-chunk loads.
pub(crate) fn has_entity_factory(kind: ferraloom_data::EntityType) -> bool {
    use ferraloom_data::EntityType;
    matches!(
        kind,
        EntityType::COW
            | EntityType::ITEM
            | EntityType::TNT
            | EntityType::FALLING_BLOCK
            | EntityType::EXPERIENCE_ORB
    )
}

/// One EntityType factory dispatcher serves cold storage and recursive live
/// loading. Constructors retain their native network-ID/UUID/RNG draw order.
pub(crate) fn create_entity_state(
    level: &crate::level::Level,
    kind: ferraloom_data::EntityType,
) -> Result<super::EntityState, EntityPersistenceError> {
    use super::{
        EntityState,
        animal::Cow,
        experience_orb::ExperienceOrb,
        item::{FallingBlockEntity, ItemEntity, PrimedTnt},
    };
    use ferraloom_data::EntityType;
    if !has_entity_factory(kind) {
        return Err(EntityPersistenceError::UnsupportedType(kind.name().into()));
    }
    let parent = level.create_entity_data(kind);
    Ok(match kind {
        EntityType::COW => EntityState::Cow(Box::new(Cow::with_level(parent, level)?)),
        EntityType::ITEM => EntityState::Item(Box::new(ItemEntity::with_parent(parent))),
        EntityType::TNT => EntityState::PrimedTnt(Box::new(PrimedTnt::with_parent(parent))),
        EntityType::FALLING_BLOCK => {
            EntityState::FallingBlock(Box::new(FallingBlockEntity::with_parent(parent)))
        }
        EntityType::EXPERIENCE_ORB => {
            EntityState::ExperienceOrb(Box::new(ExperienceOrb::with_parent(parent)))
        }
        _ => unreachable!("checked entity factory"),
    })
}

/// Reuse the canonical loader for one constructor, allowing the stream consumer
/// to register its state before recursively constructing its passengers.
pub(crate) fn load_entity_state(
    level: &crate::level::Level,
    mut tag: CompoundTag,
) -> Result<(super::EntityRef, super::EntityState), EntityPersistenceError> {
    let passengers = tag.remove("Passengers");
    let mut loaded = Vec::new();
    load_entity_tree(level, tag, &mut loaded)?;
    let (entity, mut state) = loaded.pop().expect("root constructor produced its state");
    debug_assert!(loaded.is_empty());
    if let Some(passengers) = passengers {
        state.as_entity_mut().saved.insert("Passengers", passengers);
    }
    Ok((entity, state))
}

impl EntityData {
    /// Port of Entity#load. Passengers are instantiated by the section manager.
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), EntityLoadError> {
        self.load_nbt_with_head_rotation(tag).map(|_| ())
    }

    /// Entity#load calls the derived head/body setters before wrapping yaw.
    pub(crate) fn load_nbt_with_head_rotation(
        &mut self,
        tag: CompoundTag,
    ) -> Result<f32, EntityLoadError> {
        self.load_motion(&tag);
        self.set_pos_raw(loaded_position(&tag));
        let head_rotation = self.load_rotation(&tag);
        self.load_scalar_fields(&tag);
        self.validate_loaded_state()?;
        self.set_pos(self.position);
        self.load_metadata(tag);
        Ok(head_rotation)
    }
    pub(crate) fn load_motion(&mut self, tag: &CompoundTag) {
        let motion = vector(tag, "Motion");
        let velocity = Vec3::new(
            safe_motion(motion.x),
            safe_motion(motion.y),
            safe_motion(motion.z),
        );
        if velocity.is_finite() {
            self.velocity = velocity;
        }
        self.needs_sync = true;
    }
    pub(crate) fn load_rotation(&mut self, tag: &CompoundTag) -> f32 {
        let [yaw, pitch] = tag.get("Rotation").and_then(rotation).unwrap_or([0.0, 0.0]);
        self.set_y_rot(yaw);
        self.set_x_rot(pitch);
        self.set_old_pos_and_rot();
        self.yaw
    }
    pub(crate) fn load_scalar_fields(&mut self, tag: &CompoundTag) {
        self.fall_distance = tag.get_double_or("fall_distance", 0.0);
        self.remaining_fire_ticks = i32::from(tag.get_short_or("Fire", 0));
        self.entity_data
            .set(data::AIR_SUPPLY, VarInt(tag.get_int_or("Air", 300)));
        self.on_ground = tag.get_boolean_or("OnGround", false);
        self.permanently_invulnerable = tag.get_boolean_or("Invulnerable", false);
        self.invulnerable_time = tag.get_int_or("invulnerable_time", 0);
        self.portal_cooldown = tag.get_int_or("PortalCooldown", 0);
        if let Some(uuid) = tag.get("UUID").and_then(|tag| Uuid::from_nbt(tag).ok()) {
            self.uuid = uuid;
        }
    }
    pub(crate) fn validate_loaded_state(&self) -> Result<(), EntityLoadError> {
        if !self.position.is_finite() {
            return Err(EntityLoadError::InvalidPosition);
        }
        if !self.yaw.is_finite() || !self.pitch.is_finite() {
            return Err(EntityLoadError::InvalidRotation);
        }
        Ok(())
    }
    pub(crate) fn load_metadata(&mut self, tag: CompoundTag) {
        self.set_y_rot(self.yaw % 360.0);
        self.set_x_rot(self.pitch % 360.0);
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
pub(crate) fn loaded_position(tag: &CompoundTag) -> Vec3 {
    let position = vector(tag, "Pos");
    Vec3::new(
        position.x.clamp(-30_000_512.0, 30_000_512.0),
        position.y.clamp(-20_000_000.0, 20_000_000.0),
        position.z.clamp(-30_000_512.0, 30_000_512.0),
    )
}
fn safe_motion(value: f64) -> f64 {
    if value.abs() > 10.0 { 0.0 } else { value }
}
fn vector(tag: &CompoundTag, key: &str) -> Vec3 {
    tag.get(key).and_then(optional_vector).unwrap_or(Vec3::ZERO)
}
/// Vec2.CODEC reads both components atomically, including primitive NBT arrays.
fn rotation(tag: &Tag) -> Option<[f32; 2]> {
    Some(match tag {
        Tag::List(values) if values.len() == 2 => [values[0].as_float()?, values[1].as_float()?],
        Tag::ByteArray(values) if values.len() == 2 => [f32::from(values[0]), f32::from(values[1])],
        Tag::IntArray(values) if values.len() == 2 => [values[0] as f32, values[1] as f32],
        Tag::LongArray(values) if values.len() == 2 => [values[0] as f32, values[1] as f32],
        _ => return None,
    })
}
/// Vec3.CODEC's optional three-number list, including NBT primitive arrays.
pub(super) fn optional_vector(tag: &Tag) -> Option<Vec3> {
    let values = match tag {
        Tag::List(values) if values.0.len() == 3 => [
            values.0[0].as_double()?,
            values.0[1].as_double()?,
            values.0[2].as_double()?,
        ],
        Tag::ByteArray(values) if values.len() == 3 => [
            f64::from(values[0]),
            f64::from(values[1]),
            f64::from(values[2]),
        ],
        Tag::IntArray(values) if values.len() == 3 => [
            f64::from(values[0]),
            f64::from(values[1]),
            f64::from(values[2]),
        ],
        Tag::LongArray(values) if values.len() == 3 => {
            [values[0] as f64, values[1] as f64, values[2] as f64]
        }
        _ => return None,
    };
    Some(Vec3::new(values[0], values[1], values[2]))
}

/// Vec3.CODEC through NbtOps#createDouble, whose DoubleTag.valueOf folds -0.
pub(super) fn codec_vector_tag(value: Vec3) -> Tag {
    Tag::List(ListTag(
        [value.x, value.y, value.z]
            .into_iter()
            .map(|value| Tag::Double(if value == 0.0 { 0.0 } else { value }))
            .collect(),
    ))
}

pub(super) fn vector_tag(value: Vec3) -> Tag {
    Tag::List(ListTag(vec![
        Tag::Double(value.x),
        Tag::Double(value.y),
        Tag::Double(value.z),
    ]))
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

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::EntityType;
    #[test]
    fn vanilla_load_rotation_clamping_and_motion_validation_bits() {
        let rotations = [
            [-721.25_f32, -721.25],
            [721.25, 180.0],
            [-0.0, -0.0],
            [f32::NAN, f32::INFINITY],
            [37.25, -450.125],
        ];
        let motions = [
            Vec3::new(f64::NAN, 8.0, 9.0),
            Vec3::new(10.0, -10.00001, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, -0.0, 0.25),
        ];
        let vectors = include_str!("../../tests/java/entity/vectors.tsv");
        let mut count = 0;
        for row in vectors.lines().filter(|row| row.starts_with("nbt-")) {
            let fields: Vec<_> = row.split('\t').collect();
            let index: usize = fields[1].parse().unwrap();
            let mut base = EntityData::new(EntityType::TNT, 1, 1);
            base.set_y_rot(11.25);
            base.set_x_rot(-12.5);
            base.velocity = Vec3::new(1.0, 2.0, 3.0);
            let mut tag = CompoundTag::new();
            let motion = if fields[0] == "nbt-rotation" {
                // The oracle constructs NBT through FloatTag.valueOf, which
                // canonicalizes signed zero before Entity#load sees it.
                tag.insert(
                    "Rotation",
                    Tag::List(ListTag(
                        rotations[index]
                            .map(|v| Tag::Float(if v == 0.0 { 0.0 } else { v }))
                            .to_vec(),
                    )),
                );
                Vec3::new(0.5, 0.25, -0.25)
            } else {
                motions[index]
            };
            let zero = |v| if v == 0.0 { 0.0 } else { v };
            tag.insert(
                "Motion",
                vector_tag(Vec3::new(zero(motion.x), zero(motion.y), zero(motion.z))),
            );
            base.load_nbt(tag).unwrap();
            let expected_velocity = if fields[0] == "nbt-rotation" {
                let rotation_bits: Vec<_> = fields[2]
                    .split(',')
                    .map(|v| u32::from_str_radix(v, 16).unwrap())
                    .collect();
                assert_eq!(
                    [base.yaw, base.pitch, base.old_yaw, base.old_pitch]
                        .map(f32::to_bits)
                        .as_slice(),
                    rotation_bits,
                    "{row}"
                );
                fields[3]
            } else {
                fields[2]
            };
            let velocity_bits: Vec<_> = expected_velocity
                .split(',')
                .map(|v| u64::from_str_radix(v, 16).unwrap())
                .collect();
            assert_eq!(
                [base.velocity.x, base.velocity.y, base.velocity.z]
                    .map(f64::to_bits)
                    .as_slice(),
                velocity_bits,
                "{row}"
            );
            count += 1;
        }
        assert_eq!(count, 8);
    }
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
