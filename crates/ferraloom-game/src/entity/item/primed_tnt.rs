//! Port of net.minecraft.world.entity.item.PrimedTnt.
use crate::{
    entity::{
        Entity, EntityData, EntityDataView, EntityRef, EntityState, RemovalReason, fluid,
        movement::{MovementEmission, MoverType},
        persistence,
        synched_data::EntityDataAccessor,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::codec::VarInt;
use ferraloom_util::{Uuid, Vec3, mth, random::RandomSource};

pub const FUSE: EntityDataAccessor<VarInt> = EntityDataAccessor::new(8);
pub const BLOCK_STATE: EntityDataAccessor<BlockState> = EntityDataAccessor::new(9);
#[derive(Debug)]
pub struct PrimedTnt {
    pub parent: EntityData,
    pub owner: Option<Uuid>,
    pub used_portal: bool,
    pub explosion_power: f32,
}
impl PrimedTnt {
    pub fn new(network_id: i32, seed: i64) -> Self {
        let mut parent = EntityData::new(EntityType::TNT, network_id, seed);
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
    /// level's launch-angle random draw. Entity RNG has its own supplied seed.
    pub fn new_at(level: &mut Level, position: Vec3, owner: Option<Uuid>, seed: i64) -> Self {
        let mut entity = Self::new(level.next_entity_id(), seed);
        entity.parent.set_pos(position);
        let angle = level.random().next_double() * f64::from(mth::TWO_PI);
        entity.parent.velocity = Vec3::new(
            -angle.sin() * 0.02,
            0.20000000298023224,
            -angle.cos() * 0.02,
        );
        entity.parent.old_position = position;
        entity.owner = owner;
        entity
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
    pub fn save_without_id(&self) -> CompoundTag {
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
            tag.insert("owner", Tag::IntArray(owner.to_int_array().to_vec()));
        }
        tag
    }
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), persistence::EntityLoadError> {
        self.set_fuse(i32::from(tag.get_short_or("fuse", 80)));
        self.set_block_state(
            tag.get("block_state")
                .and_then(persistence::load_block_state)
                .unwrap_or(Block::TNT.default_state()),
        );
        self.explosion_power = mth::clamp_f32(tag.get_float_or("explosion_power", 4.0), 0.0, 128.0);
        self.owner = tag
            .get_int_array("owner")
            .and_then(|v| <[i32; 4]>::try_from(v).ok())
            .map(Uuid::from_int_array);
        self.parent.load_nbt(tag)
    }
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
    fn movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn default_gravity(&self) -> f64 {
        0.04
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
        {
            let base = level.entity_mut::<EntityData>(entity)?;
            base.velocity = base.velocity.scale(f64::from(self.air_drag()));
            if base.on_ground {
                base.velocity = base.velocity.multiply(0.7, -0.5, 0.7);
            }
        }
        let fuse = level.entity::<PrimedTnt>(entity)?.fuse().wrapping_sub(1);
        level.entity_mut::<PrimedTnt>(entity)?.set_fuse(fuse);
        if fuse <= 0 {
            level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
            if level.rules().tnt_explodes {
                let tnt = level.entity::<PrimedTnt>(entity)?;
                let request = super::super::effects::EntityEffect::Explosion {
                    entity,
                    position: tnt.parent.position.add_xyz(
                        0.0,
                        f64::from(tnt.parent.dimensions.height) * 0.0625,
                        0.0,
                    ),
                    power: tnt.explosion_power,
                    owner: tnt.owner,
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
    #[test]
    fn tnt_nbt_uses_native_lowercase_fields_and_keeps_unknown_data() {
        let mut tnt = PrimedTnt::new(1, 123);
        let mut tag = tnt.save_without_id();
        tag.insert("future", Tag::String("retained".into()));
        tag.insert("fuse", Tag::Short(17));
        tag.insert("explosion_power", Tag::Float(999.0));
        let owner = Uuid::new(3, 4);
        tag.insert("owner", Tag::IntArray(owner.to_int_array().to_vec()));
        tnt.load_nbt(tag).unwrap();
        assert_eq!(tnt.fuse(), 17);
        assert_eq!(tnt.explosion_power, 128.0);
        assert_eq!(tnt.owner, Some(owner));
        let saved = tnt.save_without_id();
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
