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

/// Port of AbstractCubeMob's AgeableMob parent and its own squish/ground state.
#[derive(Debug)]
pub struct AbstractCubeMob {
    pub parent: crate::entity::AgeableMob,
    pub target_squish: f32,
    pub squish: f32,
    pub o_squish: f32,
    pub was_on_ground: bool,
}

/// Stack-local inputs captured by setSize before its metadata callback. A callback
/// may change ID_SIZE; the base tail still uses the original clamped argument,
/// while the Slime/MagmaCube override reads the live ID_SIZE afterward.
#[must_use = "finish metadata/position/dimension callbacks, then apply the size attributes"]
#[derive(Clone, Copy, Debug)]
pub struct SizeChange {
    requested: i32,
    actual: i32,
    pub metadata_changed: bool,
}
impl SizeChange {
    pub fn requested_size(self) -> i32 {
        self.requested
    }
    pub fn actual_size(self) -> i32 {
        self.actual
    }
}

impl AbstractCubeMob {
    /// Port of AbstractCubeMob#AbstractCubeMob. This is AgeableMob, not Monster:
    /// constructor XP remains zero until the first concrete setSize call.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        use crate::entity::{
            AgeableMob, Mob, PathfinderMob,
            ai::{MoveControlKind, control::MoveControl},
        };
        let mut mob = Mob::with_parent_and_metadata(parent, level.context(), |base, _| {
            base.entity_data
                .define(crate::entity::synched_data::BABY, false);
            base.entity_data
                .define(crate::entity::synched_data::AGE_LOCKED, false);
            define_synched_data(&mut base.entity_data);
            Ok(())
        })?;
        // Entity#fixupDimensions only writes dimensions/eyeHeight; it does not
        // reapply position or invoke the live growth/collision callbacks.
        let scale = mob
            .parent
            .attributes
            .value(ferraloom_data::registries::attribute::Id::SCALE)
            .expect("cube scale supplier") as f32;
        let dimensions =
            crate::entity::EntityDimensions::for_type(mob.parent.entity_type).scale(scale, scale);
        mob.parent.dimensions = dimensions.clone();
        mob.parent.eye_height = dimensions.eye_height;
        // The controller's owned family fields and tick remain a required seam
        // until the canonical MoveControl family state is released by AI.
        mob.ai.move_control = MoveControl::new(MoveControlKind::AbstractCube);
        Ok(Self {
            parent: AgeableMob {
                parent: PathfinderMob::with_parent(mob),
                age: 0,
                forced_age: 0,
            },
            target_squish: 0.0,
            squish: 0.0,
            o_squish: 0.0,
            was_on_ground: false,
        })
    }
    pub fn as_mob(&self) -> &crate::entity::Mob {
        &self.parent.parent.parent
    }
    pub fn as_mob_mut(&mut self) -> &mut crate::entity::Mob {
        &mut self.parent.parent.parent
    }
    pub fn as_living(&self) -> &crate::entity::LivingEntity {
        &self.as_mob().parent
    }
    pub fn as_living_mut(&mut self) -> &mut crate::entity::LivingEntity {
        &mut self.as_mob_mut().parent
    }
    /// Port of AbstractCubeMob#getSize.
    pub fn size(&self) -> i32 {
        self.as_living().entity_data.get(ID_SIZE).0
    }
    /// Port of AbstractCubeMob#getDefaultDimensions. Living applies SCALE after it.
    pub fn default_dimensions(&self) -> crate::entity::EntityDimensions {
        let size = self.size() as f32;
        crate::entity::EntityDimensions::for_type(self.as_living().entity_type).scale(size, size)
    }
    /// Port of AbstractCubeMob#setSize's prefix. The live caller immediately
    /// executes onSyncedDataUpdated when changed (dimensions, head/body rotation,
    /// possible water-splash RNG), then reapplyPosition and refreshDimensions.
    pub fn begin_size_change(&mut self, requested: i32) -> SizeChange {
        let actual = requested.clamp(MIN_SIZE, MAX_SIZE);
        let metadata_changed = self
            .as_living_mut()
            .entity_data
            .set(ID_SIZE, VarInt(actual));
        SizeChange {
            requested,
            actual,
            metadata_changed,
        }
    }
    /// Port of AbstractCubeMob#readAdditionalSaveData's size prefix. This runs
    /// BEFORE the Living attribute reader, not after loading canonical parents.
    pub fn begin_size_read(&mut self, tag: &ferraloom_nbt::CompoundTag) -> SizeChange {
        self.begin_size_change(tag.get_int_or("Size", 0).wrapping_add(1))
    }
    /// Port of AbstractCubeMob#setSize's attribute/health tail, after callbacks.
    pub fn apply_size_attributes(&mut self, change: SizeChange, update_health: bool) {
        use ferraloom_data::registries::attribute::Id as Attribute;
        let living = self.as_living_mut();
        living
            .attributes
            .get_instance(Attribute::MAX_HEALTH)
            .expect("cube max-health supplier")
            .set_base_value(f64::from(change.actual * change.actual));
        living
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
            .expect("cube movement supplier")
            .set_base_value(f64::from(0.2_f32 + 0.1_f32 * change.actual as f32));
        if update_health {
            living.set_health(living.max_health());
        }
    }
    /// Port of AbstractCubeMob#readAdditionalSaveData's tail, after all canonical
    /// parent reads. Slime and MagmaCube both skip Ageable's baby-only NBT fields.
    pub fn read_after_parent(&mut self, tag: &ferraloom_nbt::CompoundTag) {
        self.was_on_ground = tag.get_boolean_or("wasOnGround", false);
    }
    /// Port of AbstractCubeMob#addAdditionalSaveData's subclass tail.
    pub fn add_cube_save_data(&self, tag: &mut ferraloom_nbt::CompoundTag) {
        tag.insert("Size", ferraloom_nbt::Tag::Int(self.size() - 1));
        tag.insert(
            "wasOnGround",
            ferraloom_nbt::Tag::Byte(i8::from(self.was_on_ground)),
        );
    }
}

/// Port of Slime's empty subclass constructor and concrete setSize tail.
#[derive(Debug)]
pub struct Slime {
    pub parent: AbstractCubeMob,
}
impl Slime {
    /// Port of Slime#Slime.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Ok(Self {
            parent: AbstractCubeMob::with_level(parent, level)?,
        })
    }
    /// Port of Slime#canBeABaby. Ageable metadata exists, but its NBT is skipped.
    pub fn can_be_a_baby(&self) -> bool {
        false
    }
    /// Port of Slime#setSize after AbstractCubeMob's callbacks and attribute tail.
    pub fn apply_size_attributes(&mut self, change: SizeChange, update_health: bool) {
        self.parent.apply_size_attributes(change, update_health);
        let actual = self.parent.size();
        self.parent
            .as_living_mut()
            .attributes
            .get_instance(ferraloom_data::registries::attribute::Id::ATTACK_DAMAGE)
            .expect("slime attack supplier")
            .set_base_value(f64::from(actual));
        self.parent.as_mob_mut().xp_reward = actual;
    }
}

/// Port of MagmaCube's empty subclass constructor and concrete setSize tail.
#[derive(Debug)]
pub struct MagmaCube {
    pub parent: AbstractCubeMob,
}
impl MagmaCube {
    /// Port of MagmaCube#MagmaCube.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        Ok(Self {
            parent: AbstractCubeMob::with_level(parent, level)?,
        })
    }
    /// Port of MagmaCube#canBeABaby.
    pub fn can_be_a_baby(&self) -> bool {
        false
    }
    /// Port of MagmaCube#setSize. Armor uses the ORIGINAL argument (with Java
    /// integer overflow), even when the synchronized size was clamped to 1..127.
    pub fn apply_size_attributes(&mut self, change: SizeChange, update_health: bool) {
        use ferraloom_data::registries::attribute::Id as Attribute;
        self.parent.apply_size_attributes(change, update_health);
        let actual = self.parent.size();
        let living = self.parent.as_living_mut();
        living
            .attributes
            .get_instance(Attribute::ATTACK_DAMAGE)
            .expect("magma attack supplier")
            .set_base_value(f64::from(actual));
        living
            .attributes
            .get_instance(Attribute::ARMOR)
            .expect("magma armor supplier")
            .set_base_value(f64::from(change.requested.wrapping_mul(3)));
        self.parent.as_mob_mut().xp_reward = actual;
    }
}

#[cfg(test)]
mod tests;
