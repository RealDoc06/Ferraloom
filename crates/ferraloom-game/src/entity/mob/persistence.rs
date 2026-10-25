//! Mob's persisted fields, separate from its future navigation and AI bodies.
use crate::{
    entity::{
        EntityRef, Mob,
        player::equipment::{SLOTS, slot_name},
        synched_data,
    },
    level::{LevelContext, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{EquipmentSlot, Persistent},
};
use ferraloom_util::{BlockPos, Uuid};

/// Port of DropChances' complete EnumMap, including default entries.
#[derive(Clone, Debug)]
pub struct DropChances([f32; 8]);
impl Default for DropChances {
    fn default() -> Self {
        Self([0.085; 8])
    }
}
impl DropChances {
    pub fn by_equipment(&self, slot: EquipmentSlot) -> f32 {
        self.0[SLOTS.iter().position(|&s| s == slot).unwrap()]
    }
    /// TagValueInput keeps the unbounded-map codec's valid partial entries.
    pub(crate) fn read(tag: Option<&Tag>) -> Self {
        let mut result = Self::default();
        if let Some(Tag::Compound(tag)) = tag {
            for (i, slot) in SLOTS.into_iter().enumerate() {
                if let Some(value) = tag.get(slot_name(slot)).and_then(Tag::as_float)
                    && (0.0..=f32::MAX).contains(&value)
                    && !(value == 0.0 && value.is_sign_negative())
                {
                    result.0[i] = value;
                }
            }
        }
        result
    }
    pub(crate) fn save(&self) -> Tag {
        let mut tag = CompoundTag::new();
        for (i, slot) in SLOTS.into_iter().enumerate() {
            if self.0[i] != 0.085 {
                tag.insert(slot_name(slot), Tag::Float(self.0[i]));
            }
        }
        Tag::Compound(tag)
    }
}
/// The unresolved leash attachment decoded by LeashData.CODEC.
#[derive(Clone, Debug)]
pub enum LeashAttachment {
    Entity(Uuid),
    Fence(BlockPos),
}
/// Port of Leashable.LeashData. A bound holder is traced as a retained receiver;
/// unresolved UUIDs and fence positions do not hold an entity object yet.
#[derive(Clone, Debug)]
pub struct LeashData {
    pub holder: Option<EntityRef>,
    pub delayed: Option<LeashAttachment>,
    pub delayed_holder_id: i32,
    pub angular_momentum: f64,
}
impl LeashData {
    pub(crate) fn read(tag: Option<&Tag>) -> Option<Self> {
        let tag = tag?;
        let delayed = if let Some(uuid) = tag
            .as_compound()
            .and_then(|tag| tag.get("UUID"))
            .and_then(|tag| Uuid::from_nbt(tag).ok())
        {
            LeashAttachment::Entity(uuid)
        } else {
            LeashAttachment::Fence(block_pos(tag)?)
        };
        Some(Self {
            holder: None,
            delayed: Some(delayed),
            delayed_holder_id: 0,
            angular_momentum: 0.0,
        })
    }
}
pub(crate) fn block_pos(tag: &Tag) -> Option<BlockPos> {
    let values = match tag {
        Tag::IntArray(a) if a.len() == 3 => [a[0], a[1], a[2]],
        Tag::ByteArray(a) if a.len() == 3 => [i32::from(a[0]), i32::from(a[1]), i32::from(a[2])],
        Tag::LongArray(a) if a.len() == 3 => [a[0] as i32, a[1] as i32, a[2] as i32],
        Tag::List(a) if a.len() == 3 => [a[0].as_int()?, a[1].as_int()?, a[2].as_int()?],
        _ => return None,
    };
    Some(BlockPos::new(values[0], values[1], values[2]))
}
/// VariantUtils#getDefaultOrAny and Cow's classic-or-any sound selection use the
/// actual bound registry. An empty/missing registry is a required error.
pub(crate) fn default_variant(
    context: &LevelContext,
    registry: &str,
    preferred: &str,
) -> Result<Identifier, LevelError> {
    let registry: Identifier = registry.parse().expect("native registry key");
    let preferred: Identifier = preferred.parse().expect("native variant key");
    let entries = context.registry_context()?;
    if entries.entry_id(&registry, &preferred).is_some() {
        return Ok(preferred);
    }
    entries
        .entry_key(&registry, 0)
        .ok_or(LevelError::MissingNetworkRegistryEntry {
            registry,
            entry: preferred,
        })
}
impl Mob {
    pub fn is_no_ai(&self) -> bool {
        *self.parent.entity_data.get(synched_data::MOB_FLAGS) & 1 != 0
    }
    pub fn is_left_handed(&self) -> bool {
        *self.parent.entity_data.get(synched_data::MOB_FLAGS) & 2 != 0
    }
    pub(crate) fn read_persistent_fields(&mut self, tag: &CompoundTag) {
        self.can_pick_up_loot = tag.get_boolean_or("CanPickUpLoot", false);
        self.persistence_required = tag.get_boolean_or("PersistenceRequired", false);
        self.drop_chances = DropChances::read(tag.get("drop_chances"));
    }
    pub(crate) fn read_persistent_tail(&mut self, tag: &CompoundTag) {
        self.home_radius = tag.get_int_or("home_radius", -1);
        if self.home_radius >= 0 {
            self.home_position = tag
                .get("home_pos")
                .and_then(block_pos)
                .unwrap_or(BlockPos::ZERO);
        }
        let flags = *self.parent.entity_data.get(synched_data::MOB_FLAGS);
        self.parent.entity_data.set(
            synched_data::MOB_FLAGS,
            (flags & !2)
                | if tag.get_boolean_or("LeftHanded", false) {
                    2
                } else {
                    0
                },
        );
        self.loot_table = tag
            .get_string("DeathLootTable")
            .and_then(|key| key.to_string_checked().ok()?.parse().ok());
        self.loot_table_seed = tag.get_long_or("DeathLootTableSeed", 0);
        let flags = *self.parent.entity_data.get(synched_data::MOB_FLAGS);
        self.parent.entity_data.set(
            synched_data::MOB_FLAGS,
            (flags & !1) | i8::from(tag.get_boolean_or("NoAI", false)),
        );
    }
}
