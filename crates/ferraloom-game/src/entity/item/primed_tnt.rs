//! Port of net.minecraft.world.entity.item.PrimedTnt.
use crate::{
    entity::{
        Entity, EntityData, EntityDataView, EntityRef, EntityReference, EntityState, LivingEntity,
        RemovalReason, fluid,
        movement::{MovementEmission, MoverType},
        persistence,
        synched_data::EntityDataAccessor,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::codec::VarInt;
use ferraloom_util::{Vec3, mth, random::RandomSource};

pub const FUSE: EntityDataAccessor<VarInt> = EntityDataAccessor::new(8);
pub const BLOCK_STATE: EntityDataAccessor<BlockState> = EntityDataAccessor::new(9);
#[derive(Debug)]
pub struct PrimedTnt {
    pub parent: EntityData,
    pub owner: Option<EntityReference<LivingEntity>>,
    pub used_portal: bool,
    pub explosion_power: f32,
}
impl PrimedTnt {
    pub fn new(network_id: i32, seed: i64) -> Self {
        Self::with_parent(EntityData::new(EntityType::TNT, network_id, seed))
    }
    pub(crate) fn with_parent(mut parent: EntityData) -> Self {
        parent.blocks_building = true;
        parent.entity_data.define(FUSE, VarInt(80));
        parent
            .entity_data
            .define(BLOCK_STATE, Block::TNT.default_state());
        Self {
            parent,
            owner: None,
            used_portal: false,
            explosion_power: 4.0,
        }
    }
    /// Constructor creation order consumes the common network ID before the
    /// level's launch-angle random draw. Entity RNG comes from RandomSource.create.
    pub fn new_at(
        level: &mut Level,
        position: Vec3,
        owner: Option<EntityRef>,
    ) -> Result<Self, LevelError> {
        let mut entity = Self::with_parent(level.create_entity_data(EntityType::TNT)?);
        entity.parent.set_pos(position);
        let angle = level.random().next_double() * f64::from(mth::TWO_PI);
        entity.parent.velocity = Vec3::new(
            -angle.sin() * 0.02,
            0.20000000298023224,
            -angle.cos() * 0.02,
        );
        entity.parent.old_position = position;
        entity.owner = owner
            .map(|owner| EntityReference::of(owner, level))
            .transpose()?;
        Ok(entity)
    }
    pub fn fuse(&self) -> i32 {
        self.parent.entity_data.get(FUSE).0
    }
    pub fn set_fuse(&mut self, fuse: i32) {
        self.parent.entity_data.set(FUSE, VarInt(fuse));
    }
    pub fn block_state(&self) -> BlockState {
        *self.parent.entity_data.get(BLOCK_STATE)
    }
    pub fn set_block_state(&mut self, state: BlockState) {
        self.parent.entity_data.set(BLOCK_STATE, state);
    }
    pub fn random_short_fuse(fuse: i32, random: &mut impl RandomSource) -> i32 {
        random
            .next_int_bound((fuse / 4).max(1))
            .wrapping_add(fuse / 8)
    }
    pub fn save_without_id(&self, level: &Level) -> Result<CompoundTag, LevelError> {
        let mut tag = self.parent.save_without_id();
        tag.insert("fuse", Tag::Short(self.fuse() as i16));
        tag.insert(
            "block_state",
            persistence::save_block_state(self.block_state()),
        );
        tag.remove("explosion_power");
        if self.explosion_power != 4.0 {
            tag.insert("explosion_power", Tag::Float(self.explosion_power));
        }
        tag.remove("owner");
        if let Some(owner) = self.owner {
            tag.insert(
                "owner",
                Tag::IntArray(owner.uuid(level)?.to_int_array().to_vec()),
            );
        }
        Ok(tag)
    }
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), persistence::EntityLoadError> {
        self.parent.load_nbt(tag.clone())?;
        self.read_additional_save_data(&tag);
        Ok(())
    }
    pub(crate) fn read_additional_save_data(&mut self, tag: &CompoundTag) {
        self.set_fuse(i32::from(tag.get_short_or("fuse", 80)));
        self.set_block_state(
            tag.get("block_state")
                .and_then(persistence::load_block_state)
                .unwrap_or(Block::TNT.default_state()),
        );
        self.explosion_power = mth::clamp_f32(tag.get_float_or("explosion_power", 4.0), 0.0, 128.0);
        self.owner = crate::entity::living::damage_state::read_reference(tag, "owner");
    }
}
/// Port of PrimedTnt#getOwner, retaining the LivingEntity type restriction.
pub fn get_owner(entity: EntityRef, level: &mut Level) -> Result<Option<EntityRef>, LevelError> {
    let Some(mut reference) = level.entity::<PrimedTnt>(entity)?.owner else {
        return Ok(None);
    };
    let owner = reference.resolve(level)?;
    level.entity_mut::<PrimedTnt>(entity)?.owner = Some(reference);
    Ok(owner)
}
impl EntityDataView for PrimedTnt {
    fn view(state: &EntityState) -> Option<&Self> {
        if let EntityState::PrimedTnt(tnt) = state {
            Some(tnt)
        } else {
            None
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        if let EntityState::PrimedTnt(tnt) = state {
            Some(tnt)
        } else {
            None
        }
    }
}
pub struct PrimedTntBehavior;
impl Entity for PrimedTntBehavior {
    fn get_owner(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<Option<EntityRef>, LevelError> {
        get_owner(entity, level)
    }

    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<crate::entity::EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
        if let Some(tnt) = PrimedTnt::view(state) {
            references.extend(tnt.owner.and_then(EntityReference::cached_entity));
        }
    }

    fn movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn default_gravity(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        Ok(0.04)
    }
    /// PrimedTnt deliberately does not call Entity#tick/baseTick.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let gravity = self.effective_gravity(entity, level)?;
        if gravity != 0.0 {
            level.entity_mut::<EntityData>(entity)?.velocity.y -= gravity;
        }
        let movement = level.entity::<EntityData>(entity)?.velocity;
        self.move_by(entity, level, MoverType::SelfMovement, movement)?;
        self.apply_effects_from_blocks(entity, level)?;
        let air = self.air_drag(entity, level)?;
        {
            let base = level.entity_mut::<EntityData>(entity)?;
            base.velocity = base.velocity.scale(f64::from(air));
            if base.on_ground {
                base.velocity = base.velocity.multiply(0.7, -0.5, 0.7);
            }
        }
        let fuse = level.entity::<PrimedTnt>(entity)?.fuse().wrapping_sub(1);
        level.entity_mut::<PrimedTnt>(entity)?.set_fuse(fuse);
        if fuse <= 0 {
            level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
            if level.rules().tnt_explodes {
                let owner = get_owner(entity, level)?
                    .map(|owner| level.entity::<EntityData>(owner).map(|owner| owner.uuid))
                    .transpose()?;
                let tnt = level.entity::<PrimedTnt>(entity)?;
                let request = super::super::effects::EntityEffect::Explosion {
                    entity,
                    position: tnt.parent.position.add_xyz(
                        0.0,
                        f64::from(tnt.parent.dimensions.height) * 0.0625,
                        0.0,
                    ),
                    power: tnt.explosion_power,
                    owner,
                    used_portal: tnt.used_portal,
                };
                level.emit_entity_effect(request);
            }
        } else {
            fluid::update_fluid_interaction(self, entity, level)?;
        }
        let base = level.entity_mut::<EntityData>(entity)?;
        base.requires_precise_position = base.horizontal_collision || base.vertical_collision;
        Ok(())
    }
    fn hurt_server(
        &self,
        _entity: EntityRef,
        _level: &mut Level,
        _source: crate::entity::DamageSource,
        _damage: f32,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::Uuid;
    #[test]
    fn vanilla_tnt_launch_uses_level_random_after_canonical_construction() {
        let mut rows = 0;
        for row in include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|row| row.starts_with("tnt-launch\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
            let mut level = {
                let mut fixture_level = Level::new(
                    crate::level::Region::new(&BEHAVIORS),
                    crate::level::LevelContext::fixture(0, 63),
                );
                fixture_level.install_shared(Box::default());
                fixture_level
            };
            level.random().set_seed(fields[1].parse().unwrap());
            let tnt = PrimedTnt::new_at(&mut level, Vec3::new(1.0, 64.0, 1.0), None).unwrap();
            let expected: Vec<_> = fields[2]
                .split(',')
                .map(|v| u64::from_str_radix(v, 16).unwrap())
                .collect();
            assert_eq!(
                [
                    tnt.parent.velocity.x,
                    tnt.parent.velocity.y,
                    tnt.parent.velocity.z
                ]
                .map(f64::to_bits)
                .as_slice(),
                expected.as_slice(),
                "{row}"
            );
            assert_eq!(tnt.parent.network_id, 1);
            assert_eq!(level.next_entity_id().unwrap(), 2);
            rows += 1;
        }
        assert_eq!(rows, 3);
    }
    #[test]
    fn tnt_nbt_uses_native_lowercase_fields_and_keeps_unknown_data() {
        let mut tnt = PrimedTnt::new(1, 123);
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let level = {
            let mut fixture_level = Level::new(
                crate::level::Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let mut tag = tnt.save_without_id(&level).unwrap();
        tag.insert("future", Tag::String("retained".into()));
        tag.insert("fuse", Tag::Short(17));
        tag.insert("explosion_power", Tag::Float(999.0));
        let owner = Uuid::new(3, 4);
        tag.insert("owner", Tag::IntArray(owner.to_int_array().to_vec()));
        tnt.load_nbt(tag).unwrap();
        assert_eq!(tnt.fuse(), 17);
        assert_eq!(tnt.explosion_power, 128.0);
        assert_eq!(tnt.owner.unwrap().uuid(&level).unwrap(), owner);
        let saved = tnt.save_without_id(&level).unwrap();
        assert_eq!(
            saved.get_string("future").unwrap().to_string_lossy(),
            "retained"
        );
        assert_eq!(
            saved.get("block_state"),
            Some(&Tag::String("minecraft:tnt".into()))
        );
        assert!(!saved.contains_key("Fuse"));
    }
}
