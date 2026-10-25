//! Typed access to Entity's metadata. Wire values use the protocol serializers.
use std::marker::PhantomData;

use ferraloom_protocol::{
    chat::Component,
    codec::VarInt,
    entity_data::{DataValue, EntityDataValue, Pose},
};

/// Port of EntityDataAccessor. The value type is checked without casts.
#[derive(Debug)]
pub struct EntityDataAccessor<T> {
    id: u8,
    value: PhantomData<fn() -> T>,
}
impl<T> Copy for EntityDataAccessor<T> {}
impl<T> Clone for EntityDataAccessor<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> EntityDataAccessor<T> {
    pub const fn new(id: u8) -> Self {
        assert!(id < 255);
        Self {
            id,
            value: PhantomData,
        }
    }
    pub const fn id(self) -> u8 {
        self.id
    }
}

pub trait SynchedValue: Clone + PartialEq {
    const IDENTITY_EQUALITY: bool = false;
    fn from_value(value: &EntityDataValue) -> Option<&Self>;
    fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self>;
    fn into_value(self) -> EntityDataValue;
    fn same_value(&self, other: &Self) -> bool {
        self == other
    }
}
macro_rules! value {
    ($ty:ty, $variant:ident) => {
        impl SynchedValue for $ty {
            fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self> {
                if let EntityDataValue::$variant(value) = value {
                    Some(value)
                } else {
                    None
                }
            }
            fn from_value(value: &EntityDataValue) -> Option<&Self> {
                if let EntityDataValue::$variant(value) = value {
                    Some(value)
                } else {
                    None
                }
            }
            fn into_value(self) -> EntityDataValue {
                EntityDataValue::$variant(self)
            }
        }
    };
}
value!(i8, Byte);
value!(VarInt, Int);
value!(bool, Boolean);
value!(ferraloom_protocol::component::CowVariant, CowVariant);
value!(
    ferraloom_protocol::component::CowSoundVariant,
    CowSoundVariant
);
value!(Pose, Pose);
value!(Option<Box<Component>>, OptionalComponent);
value!(ferraloom_data::BlockState, BlockState);
value!(ferraloom_util::BlockPos, BlockPos);
value!(Option<ferraloom_util::BlockPos>, OptionalBlockPos);
value!(Vec<ferraloom_protocol::entity_data::Particle>, Particles);
value!(ferraloom_protocol::entity_data::HumanoidArm, HumanoidArm);
value!(
    ferraloom_protocol::entity_data::OptionalUnsignedInt,
    OptionalUnsignedInt
);

impl SynchedValue for f32 {
    fn from_value(value: &EntityDataValue) -> Option<&Self> {
        if let EntityDataValue::Float(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self> {
        if let EntityDataValue::Float(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn into_value(self) -> EntityDataValue {
        EntityDataValue::Float(self)
    }
    /// Float#equals uses floatToIntBits: signed zero differs, NaN payloads agree.
    fn same_value(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits() || self.is_nan() && other.is_nan()
    }
}

impl SynchedValue for ferraloom_protocol::component::ItemStack {
    const IDENTITY_EQUALITY: bool = true;
    fn from_value(value: &EntityDataValue) -> Option<&Self> {
        if let EntityDataValue::ItemStack(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self> {
        if let EntityDataValue::ItemStack(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn into_value(self) -> EntityDataValue {
        EntityDataValue::ItemStack(self)
    }
    /// ItemStack inherits object identity equality. An owned setter argument is
    /// another stack; None denotes the shared EMPTY singleton. Mutating the held
    /// stack through get_mut does not call set or dirty the DataItem in vanilla.
    fn same_value(&self, other: &Self) -> bool {
        self.0.is_none() && other.0.is_none()
    }
}

pub const SHARED_FLAGS: EntityDataAccessor<i8> = EntityDataAccessor::new(0);
pub const AIR_SUPPLY: EntityDataAccessor<VarInt> = EntityDataAccessor::new(1);
pub const CUSTOM_NAME: EntityDataAccessor<Option<Box<Component>>> = EntityDataAccessor::new(2);
pub const CUSTOM_NAME_VISIBLE: EntityDataAccessor<bool> = EntityDataAccessor::new(3);
pub const SILENT: EntityDataAccessor<bool> = EntityDataAccessor::new(4);
pub const NO_GRAVITY: EntityDataAccessor<bool> = EntityDataAccessor::new(5);
pub const POSE: EntityDataAccessor<Pose> = EntityDataAccessor::new(6);
pub const TICKS_FROZEN: EntityDataAccessor<VarInt> = EntityDataAccessor::new(7);

// Accessor IDs follow the immediate Java parent, so sibling classes reuse IDs.
pub const LIVING_ENTITY_FLAGS: EntityDataAccessor<i8> = EntityDataAccessor::new(8);
pub const HEALTH: EntityDataAccessor<f32> = EntityDataAccessor::new(9);
pub const EFFECT_PARTICLES: EntityDataAccessor<Vec<ferraloom_protocol::entity_data::Particle>> =
    EntityDataAccessor::new(10);
pub const EFFECT_AMBIENCE: EntityDataAccessor<bool> = EntityDataAccessor::new(11);
pub const ARROW_COUNT: EntityDataAccessor<VarInt> = EntityDataAccessor::new(12);
pub const STINGER_COUNT: EntityDataAccessor<VarInt> = EntityDataAccessor::new(13);
pub const SLEEPING_POS: EntityDataAccessor<Option<ferraloom_util::BlockPos>> =
    EntityDataAccessor::new(14);
pub const PLAYER_MAIN_HAND: EntityDataAccessor<ferraloom_protocol::entity_data::HumanoidArm> =
    EntityDataAccessor::new(15);
pub const PLAYER_MODEL_CUSTOMISATION: EntityDataAccessor<i8> = EntityDataAccessor::new(16);
pub const PLAYER_ABSORPTION: EntityDataAccessor<f32> = EntityDataAccessor::new(17);
pub const PLAYER_SCORE: EntityDataAccessor<VarInt> = EntityDataAccessor::new(18);
pub const SHOULDER_PARROT_LEFT: EntityDataAccessor<
    ferraloom_protocol::entity_data::OptionalUnsignedInt,
> = EntityDataAccessor::new(19);
pub const SHOULDER_PARROT_RIGHT: EntityDataAccessor<
    ferraloom_protocol::entity_data::OptionalUnsignedInt,
> = EntityDataAccessor::new(20);
pub const MOB_FLAGS: EntityDataAccessor<i8> = EntityDataAccessor::new(15);
pub const BABY: EntityDataAccessor<bool> = EntityDataAccessor::new(16);
pub const AGE_LOCKED: EntityDataAccessor<bool> = EntityDataAccessor::new(17);
pub const COW_VARIANT: EntityDataAccessor<ferraloom_protocol::component::CowVariant> =
    EntityDataAccessor::new(18);
pub const COW_SOUND_VARIANT: EntityDataAccessor<ferraloom_protocol::component::CowSoundVariant> =
    EntityDataAccessor::new(19);

#[derive(Debug)]
struct DataItem {
    initial: EntityDataValue,
    value: EntityDataValue,
    dirty: bool,
    same_value: fn(&EntityDataValue, &EntityDataValue) -> bool,
    initial_is_current: bool,
}

/// Port of SynchedEntityData. Index order is both accessor and packet order.
#[derive(Debug)]
pub struct SynchedEntityData {
    items: Vec<DataItem>,
}
impl Default for SynchedEntityData {
    fn default() -> Self {
        let mut data = Self { items: Vec::new() };
        data.define(SHARED_FLAGS, 0);
        data.define(AIR_SUPPLY, VarInt(300));
        data.define(CUSTOM_NAME, None);
        data.define(CUSTOM_NAME_VISIBLE, false);
        data.define(SILENT, false);
        data.define(NO_GRAVITY, false);
        data.define(POSE, Pose::Standing);
        data.define(TICKS_FROZEN, VarInt(0));
        data
    }
}
impl SynchedEntityData {
    pub fn define<T: SynchedValue>(&mut self, accessor: EntityDataAccessor<T>, value: T) {
        assert_eq!(
            usize::from(accessor.id),
            self.items.len(),
            "metadata must be defined in accessor order"
        );
        let value = value.into_value();
        self.items.push(DataItem {
            initial: value.clone(),
            value,
            dirty: false,
            same_value: |a, b| {
                T::from_value(a)
                    .unwrap()
                    .same_value(T::from_value(b).unwrap())
            },
            initial_is_current: T::IDENTITY_EQUALITY,
        });
    }
    pub fn get<T: SynchedValue>(&self, accessor: EntityDataAccessor<T>) -> &T {
        T::from_value(&self.items[usize::from(accessor.id)].value).expect("metadata accessor type")
    }
    /// Java returns mutable stack references without dirtying the DataItem.
    /// Call set_forced when an owning setter explicitly requests synchronization.
    pub fn get_mut<T: SynchedValue>(&mut self, accessor: EntityDataAccessor<T>) -> &mut T {
        T::from_value_mut(&mut self.items[usize::from(accessor.id)].value)
            .expect("metadata accessor type")
    }
    /// Returns whether the onSyncedDataUpdated callback must run at this call site.
    pub fn set<T: SynchedValue>(&mut self, accessor: EntityDataAccessor<T>, value: T) -> bool {
        self.set_forced(accessor, value, false)
    }
    pub fn set_forced<T: SynchedValue>(
        &mut self,
        accessor: EntityDataAccessor<T>,
        value: T,
        force: bool,
    ) -> bool {
        let item = &mut self.items[usize::from(accessor.id)];
        let current = T::from_value(&item.value).expect("metadata accessor type");
        if force || !current.same_value(&value) {
            item.value = value.into_value();
            item.dirty = true;
            item.initial_is_current = false;
            true
        } else {
            false
        }
    }
    pub fn is_dirty(&self) -> bool {
        self.items.iter().any(|item| item.dirty)
    }
    pub fn pack_dirty(&mut self) -> Option<Vec<DataValue>> {
        if !self.is_dirty() {
            return None;
        }
        Some(
            self.items
                .iter_mut()
                .enumerate()
                .filter_map(|(id, item)| {
                    if !item.dirty {
                        return None;
                    }
                    item.dirty = false;
                    Some(DataValue {
                        id: id as u8,
                        value: item.value.clone(),
                    })
                })
                .collect(),
        )
    }
    pub fn non_default_values(&self) -> Option<Vec<DataValue>> {
        let values: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                !item.initial_is_current && !(item.same_value)(&item.initial, &item.value)
            })
            .map(|(id, item)| DataValue {
                id: id as u8,
                value: item.value.clone(),
            })
            .collect();
        (!values.is_empty()).then_some(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_player_metadata_follows_living_avatar_and_player_accessor_defaults() {
        use crate::entity::player::{GameType, ServerPlayer};
        let player = ServerPlayer::new(
            ferraloom_util::Uuid::new(1, 2),
            "metadata".into(),
            1,
            ferraloom_util::Vec3::ZERO,
            GameType::Survival,
        );
        let data = &player.entity_data;
        let actual: Vec<_> = data
            .items
            .iter()
            .enumerate()
            .map(|(id, item)| {
                let value = match &item.value {
                    EntityDataValue::Byte(value) => value.to_string(),
                    EntityDataValue::Int(value) => value.0.to_string(),
                    EntityDataValue::Float(value) => format!("{:x}", value.to_bits()),
                    EntityDataValue::Boolean(value) => value.to_string(),
                    EntityDataValue::Pose(value) => (*value as i32).to_string(),
                    EntityDataValue::HumanoidArm(value) => (*value as i32).to_string(),
                    EntityDataValue::OptionalComponent(None)
                    | EntityDataValue::OptionalBlockPos(None)
                    | EntityDataValue::OptionalUnsignedInt(
                        ferraloom_protocol::entity_data::OptionalUnsignedInt(None),
                    ) => "none".into(),
                    EntityDataValue::Particles(values) if values.is_empty() => "[]".into(),
                    other => panic!("unexpected default metadata: {other:?}"),
                };
                format!(
                    "living-data\t{id}\t{}\t{value}\t{}",
                    item.value.serializer() as i32,
                    item.dirty
                )
            })
            .collect();
        let expected: Vec<_> = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("living-data\t"))
            .collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn real_health_setters_use_attribute_maximum_and_java_float_metadata_equality() {
        use crate::entity::player::{GameType, ServerPlayer};
        let mut player = ServerPlayer::new(
            ferraloom_util::Uuid::new(1, 2),
            "metadata".into(),
            1,
            ferraloom_util::Vec3::ZERO,
            GameType::Survival,
        );
        player.entity_data.pack_dirty();
        player
            .attributes
            .get_instance(ferraloom_data::registries::attribute::Id::MAX_HEALTH)
            .unwrap()
            .set_base_value(30.0);
        let mut actual = Vec::new();
        for (index, health) in [
            20.0,
            30.0,
            -0.0,
            f32::NAN,
            f32::from_bits(0x7f80_0001),
            f32::INFINITY,
            f32::NEG_INFINITY,
            7.5,
        ]
        .into_iter()
        .enumerate()
        {
            player.set_health(health);
            let bits = player.health().to_bits();
            let dirty = player.entity_data.pack_dirty().is_some();
            actual.push(format!("living-health\t{index}\t{bits:x}\t{dirty}"));
        }
        let expected: Vec<_> = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("living-health\t"))
            .collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn dirty_metadata_preserves_accessor_order_and_forced_updates() {
        let mut data = SynchedEntityData::default();
        assert!(data.pack_dirty().is_none());
        data.set(TICKS_FROZEN, VarInt(140));
        data.set(SHARED_FLAGS, 2);
        assert!(!data.set(SHARED_FLAGS, 2));
        assert_eq!(
            data.pack_dirty()
                .unwrap()
                .iter()
                .map(|v| v.id)
                .collect::<Vec<_>>(),
            [0, 7]
        );
        assert!(data.pack_dirty().is_none());
        data.set_forced(SHARED_FLAGS, 2, true);
        assert_eq!(data.pack_dirty().unwrap().len(), 1);
        data.set(SHARED_FLAGS, 0);
        assert_eq!(data.non_default_values().unwrap()[0].id, 7);
    }

    #[test]
    fn vanilla_boxed_float_equality_preserves_signed_zero_and_canonical_nan_comparison() {
        const FLOAT: EntityDataAccessor<f32> = EntityDataAccessor::new(8);
        let values = [
            0.0,
            -0.0,
            -0.0,
            f32::NAN,
            f32::from_bits(0x7f80_0001),
            1.0,
            1.0,
        ];
        let mut data = SynchedEntityData::default();
        data.define(FLOAT, 0.0);
        let mut rows = 0;
        for row in include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|row| row.starts_with("synched-float\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            data.set(FLOAT, values[fields[1].parse::<usize>().unwrap()]);
            assert_eq!(data.pack_dirty().is_some().to_string(), fields[2], "{row}");
            assert_eq!(
                format!("{:x}", data.get(FLOAT).to_bits()),
                fields[3],
                "{row}"
            );
            assert_eq!(
                data.non_default_values().is_some().to_string(),
                fields[4],
                "{row}"
            );
            rows += 1;
        }
        assert_eq!(rows, 7);
    }

    #[test]
    fn vanilla_stack_setters_compare_identity_and_held_mutations_do_not_dirty_metadata() {
        use ferraloom_data::Item;
        use ferraloom_protocol::component::ItemStack;
        const STACK: EntityDataAccessor<ItemStack> = EntityDataAccessor::new(8);
        let stone = || ItemStack::new(Item::STONE, 3, Default::default());
        let mut data = SynchedEntityData::default();
        data.define(STACK, ItemStack::default());
        let mut changes = Vec::new();
        data.set(STACK, stone());
        changes.push(data.pack_dirty().is_some());
        data.set(STACK, stone());
        changes.push(data.pack_dirty().is_some());
        // Native set(existing reference) keeps the object already held here.
        let _held = data.get(STACK);
        changes.push(data.pack_dirty().is_some());
        data.get_mut(STACK).0.as_mut().unwrap().count = 4;
        changes.push(data.pack_dirty().is_some());
        let held = data.get(STACK).clone();
        data.set_forced(STACK, held, true);
        changes.push(data.pack_dirty().is_some());
        data.set(STACK, ItemStack::default());
        changes.push(data.pack_dirty().is_some());
        data.set(STACK, ItemStack::default());
        changes.push(data.pack_dirty().is_some());
        let expected = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .find_map(|row| row.strip_prefix("synched-stack\t"))
            .unwrap();
        let actual = format!(
            "{}\t{}",
            changes
                .iter()
                .map(bool::to_string)
                .collect::<Vec<_>>()
                .join(","),
            data.non_default_values().is_some()
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn initial_stack_mutations_keep_the_initial_object_identity() {
        use ferraloom_data::Item;
        use ferraloom_protocol::component::ItemStack;
        const STACK: EntityDataAccessor<ItemStack> = EntityDataAccessor::new(8);
        let mut data = SynchedEntityData::default();
        data.define(STACK, ItemStack::new(Item::STONE, 1, Default::default()));
        assert!(data.non_default_values().is_none());
        data.get_mut(STACK).0.as_mut().unwrap().count = 2;
        let mutated = data.non_default_values().is_some();
        let copy = data.get(STACK).clone();
        data.set(STACK, copy);
        let expected = include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .find_map(|row| row.strip_prefix("synched-prototype\t"))
            .unwrap();
        assert_eq!(
            format!("{mutated},{}", data.non_default_values().is_some()),
            expected
        );
    }
}
