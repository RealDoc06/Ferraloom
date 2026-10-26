//! ZombieVillager's own synchronized fields; profession selection uses the entity RNG.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData, SynchedValue};
use ferraloom_protocol::entity_data::{EntityDataValue, VillagerData};

/// Port of ZombieVillager#DATA_CONVERTING_ID.
pub const DATA_CONVERTING_ID: EntityDataAccessor<bool> = EntityDataAccessor::new(19);
/// Port of ZombieVillager#DATA_VILLAGER_DATA.
pub const DATA_VILLAGER_DATA: EntityDataAccessor<VillagerData> = EntityDataAccessor::new(20);
/// Port of ZombieVillager#DATA_VILLAGER_DATA_FINALIZED.
pub const DATA_VILLAGER_DATA_FINALIZED: EntityDataAccessor<bool> = EntityDataAccessor::new(21);

impl SynchedValue for VillagerData {
    fn from_value(value: &EntityDataValue) -> Option<&Self> {
        if let EntityDataValue::VillagerData(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self> {
        if let EntityDataValue::VillagerData(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn into_value(self) -> EntityDataValue {
        EntityDataValue::VillagerData(self)
    }
}

/// Port of ZombieVillager#defineSynchedData. The caller supplies the result of
/// initializeZombieVillagerData at the native Entity-constructor RNG draw point.
pub fn define_synched_data(data: &mut SynchedEntityData, initial: VillagerData) {
    data.define(DATA_CONVERTING_ID, false);
    data.define(DATA_VILLAGER_DATA, initial);
    data.define(DATA_VILLAGER_DATA_FINALIZED, false);
}
