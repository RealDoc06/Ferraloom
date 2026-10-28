//! Zombie-family metadata and spawn-group inputs. Canonical parents belong to Mob.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData};
use ferraloom_protocol::codec::VarInt;

pub mod villager;

/// Port of Zombie#DATA_BABY_ID.
pub const DATA_BABY_ID: EntityDataAccessor<bool> = EntityDataAccessor::new(16);
/// Port of Zombie#DATA_SPECIAL_TYPE_ID, retained even though no native setter uses it.
pub const DATA_SPECIAL_TYPE_ID: EntityDataAccessor<VarInt> = EntityDataAccessor::new(17);
/// Port of Zombie#DATA_DROWNED_CONVERSION_ID.
pub const DATA_DROWNED_CONVERSION_ID: EntityDataAccessor<bool> = EntityDataAccessor::new(18);

/// Port of Zombie#defineSynchedData's subclass portion, after Mob's accessor 15.
pub fn define_synched_data(data: &mut SynchedEntityData) {
    data.define(DATA_BABY_ID, false);
    data.define(DATA_SPECIAL_TYPE_ID, VarInt(0));
    data.define(DATA_DROWNED_CONVERSION_ID, false);
}

/// Port of Zombie.ZombieGroupData. One value is shared by the native spawn group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZombieGroupData {
    pub is_baby: bool,
    pub can_spawn_jockey: bool,
}
