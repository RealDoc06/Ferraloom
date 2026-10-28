//! Spider metadata is inherited by CaveSpider without another accessor.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData};

/// Port of Spider#DATA_FLAGS_ID; bit 0 represents climbing.
pub const DATA_FLAGS_ID: EntityDataAccessor<i8> = EntityDataAccessor::new(16);

/// Port of Spider#defineSynchedData's own field.
pub fn define_synched_data(data: &mut SynchedEntityData) {
    data.define(DATA_FLAGS_ID, 0);
}
