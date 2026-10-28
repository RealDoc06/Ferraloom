//! AbstractCubeMob extends AgeableMob in 26.3, independently of Monster.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData};
use ferraloom_protocol::codec::VarInt;

/// Port of AbstractCubeMob#ID_SIZE, after AgeableMob's baby and age-locked fields.
pub const ID_SIZE: EntityDataAccessor<VarInt> = EntityDataAccessor::new(18);
/// Port of AbstractCubeMob#MIN_SIZE.
pub const MIN_SIZE: i32 = 1;
/// Port of AbstractCubeMob#MAX_SIZE.
pub const MAX_SIZE: i32 = 127;
/// Port of AbstractCubeMob#MAX_NATURAL_SIZE.
pub const MAX_NATURAL_SIZE: i32 = 4;

/// Port of AbstractCubeMob#defineSynchedData's own field.
pub fn define_synched_data(data: &mut SynchedEntityData) {
    data.define(ID_SIZE, VarInt(MIN_SIZE));
}
