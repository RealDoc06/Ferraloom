//! Zombie-family metadata and spawn-group inputs. Canonical parents belong to Mob.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData};
use ferraloom_protocol::codec::VarInt;

pub mod drowned;
pub mod villager;
pub mod zombified_piglin;

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

/// Port of Zombie's subclass state. The retained BreakDoorGoal is supplied by
/// the AI owner; navigation/goal callbacks remain required until that port.
#[derive(Debug)]
pub struct Zombie {
    pub parent: super::Monster,
    pub(crate) can_break_doors: bool,
    pub drowning_tracker: DrowningTracker,
}

/// Port of ConversionTracker's Zombie-specific counters. Converting lives only
/// in DATA_DROWNED_CONVERSION_ID; the afflicted predicate is read from the world.
#[derive(Debug, Default)]
pub struct DrowningTracker {
    pub affliction_time: i32,
    pub conversion_time: i32,
}

impl Zombie {
    /// Port of Zombie#Zombie. Derived metadata runs before Living's yaw draw.
    /// This constructs state; concrete registerGoals remains a required AI seam.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Self::with_metadata(parent, level, |_, _| Ok(()))
    }
    pub(super) fn with_metadata(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
        derived: impl FnOnce(
            &mut crate::entity::EntityData,
            &crate::level::LevelContext,
        ) -> Result<(), crate::level::LevelError>,
    ) -> Result<Self, crate::level::LevelError> {
        let mob = crate::entity::Mob::with_parent_and_metadata(
            parent,
            level.context(),
            |base, context| {
                define_synched_data(&mut base.entity_data);
                derived(base, context)
            },
        )?;
        Ok(Self {
            parent: super::Monster::with_parent(mob),
            can_break_doors: false,
            drowning_tracker: Default::default(),
        })
    }
    pub fn as_mob(&self) -> &crate::entity::Mob {
        self.parent.as_mob()
    }
    pub fn as_mob_mut(&mut self) -> &mut crate::entity::Mob {
        self.parent.as_mob_mut()
    }
    pub fn as_living(&self) -> &crate::entity::LivingEntity {
        self.parent.as_living()
    }
    pub fn as_living_mut(&mut self) -> &mut crate::entity::LivingEntity {
        self.parent.as_living_mut()
    }
    pub fn is_baby(&self) -> bool {
        *self.as_living().entity_data.get(DATA_BABY_ID)
    }
    pub fn can_break_doors(&self) -> bool {
        self.can_break_doors
    }
    pub fn is_under_water_converting(&self) -> bool {
        *self.as_living().entity_data.get(DATA_DROWNED_CONVERSION_ID)
    }
    /// Port of Zombie#setBaby's metadata step. A live caller immediately runs
    /// refreshDimensions on change, ends that callback, then updates speed.
    pub fn set_baby_metadata(&mut self, baby: bool) -> bool {
        let changed = self.is_baby() != baby;
        self.as_living_mut().entity_data.set(DATA_BABY_ID, baby);
        changed
    }
    /// Port of Zombie#setBaby's server attribute step, AFTER the metadata callback.
    pub fn update_baby_speed(&mut self) {
        use crate::entity::ai::attributes::{AttributeModifier, AttributeOperation};
        let baby = self.is_baby();
        let mut speed = self
            .as_living_mut()
            .attributes
            .get_instance(ferraloom_data::registries::attribute::Id::MOVEMENT_SPEED)
            .expect("Zombie movement speed supplier");
        let id = "minecraft:baby".parse().expect("native modifier id");
        speed.remove_modifier(&id);
        if baby {
            speed
                .add_transient_modifier(std::sync::Arc::new(AttributeModifier {
                    id,
                    amount: 0.5.into(),
                    operation: AttributeOperation::AddMultipliedBase,
                }))
                .expect("removed baby modifier");
        }
    }
    /// Port of ConversionTracker#readAdditionalSaveData, after setBaby and
    /// setCanBreakDoors have completed, including their synchronous callbacks.
    pub fn read_drowning_data(&mut self, tag: &ferraloom_nbt::CompoundTag) {
        self.drowning_tracker.affliction_time = tag.get_int_or("InWaterTime", 0);
        let time = tag.get_int_or("DrownedConversionTime", -1);
        if time != -1 {
            self.drowning_tracker.conversion_time = time;
        }
        self.as_living_mut()
            .entity_data
            .set(DATA_DROWNED_CONVERSION_ID, time != -1);
    }
    /// Port of Zombie#addAdditionalSaveData's subclass tail. The caller evaluates
    /// isEyeInFluid(WATER) at this point, after saving the canonical parents.
    pub fn add_zombie_save_data(&self, tag: &mut ferraloom_nbt::CompoundTag, eye_in_water: bool) {
        use ferraloom_nbt::Tag;
        tag.insert("IsBaby", Tag::Byte(i8::from(self.is_baby())));
        tag.insert("CanBreakDoors", Tag::Byte(i8::from(self.can_break_doors())));
        tag.insert(
            "DrownedConversionTime",
            Tag::Int(if self.is_under_water_converting() {
                self.drowning_tracker.conversion_time
            } else {
                -1
            }),
        );
        tag.insert(
            "InWaterTime",
            Tag::Int(if eye_in_water {
                self.drowning_tracker.affliction_time
            } else {
                -1
            }),
        );
    }
}

/// Port of Husk's empty subclass constructor; all persisted fields are Zombie's.
#[derive(Debug)]
pub struct Husk {
    pub parent: Zombie,
}
impl Husk {
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Ok(Self {
            parent: Zombie::with_level(parent, level)?,
        })
    }
}
