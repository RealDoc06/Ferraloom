//! Typed access to Entity's metadata. Wire values use the protocol serializers.
use std::marker::PhantomData;

use ferraloom_protocol::{
    chat::Component,
    codec::VarInt,
    entity_data::{DataValue, EntityDataValue, Pose},
};

/// Port of EntityDataAccessor. The value type is checked without casts.
#[derive(Clone, Copy, Debug)]
pub struct EntityDataAccessor<T> {
    id: u8,
    value: PhantomData<fn() -> T>,
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
    fn from_value(value: &EntityDataValue) -> Option<&Self>;
    fn into_value(self) -> EntityDataValue;
}
macro_rules! value {
    ($ty:ty, $variant:ident) => {
        impl SynchedValue for $ty {
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
value!(Pose, Pose);
value!(Option<Box<Component>>, OptionalComponent);

pub const SHARED_FLAGS: EntityDataAccessor<i8> = EntityDataAccessor::new(0);
pub const AIR_SUPPLY: EntityDataAccessor<VarInt> = EntityDataAccessor::new(1);
pub const CUSTOM_NAME: EntityDataAccessor<Option<Box<Component>>> = EntityDataAccessor::new(2);
pub const CUSTOM_NAME_VISIBLE: EntityDataAccessor<bool> = EntityDataAccessor::new(3);
pub const SILENT: EntityDataAccessor<bool> = EntityDataAccessor::new(4);
pub const NO_GRAVITY: EntityDataAccessor<bool> = EntityDataAccessor::new(5);
pub const POSE: EntityDataAccessor<Pose> = EntityDataAccessor::new(6);
pub const TICKS_FROZEN: EntityDataAccessor<VarInt> = EntityDataAccessor::new(7);

#[derive(Debug)]
struct DataItem {
    initial: EntityDataValue,
    value: EntityDataValue,
    dirty: bool,
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
        });
    }
    pub fn get<T: SynchedValue>(&self, accessor: EntityDataAccessor<T>) -> &T {
        T::from_value(&self.items[usize::from(accessor.id)].value).expect("metadata accessor type")
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
        if force || *current != value {
            item.value = value.into_value();
            item.dirty = true;
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
            .filter(|(_, item)| item.initial != item.value)
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
}
