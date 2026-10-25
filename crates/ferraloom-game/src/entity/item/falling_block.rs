//! Port of FallingBlockEntity's sand/gravel physics, landing and persistence.
use crate::{
    block::{DirectionalPlaceContext, LevelReaderRef, fluid::legacy_block},
    entity::{
        Entity, EntityData, EntityDataView, EntityRef, EntityState,
        effects::EntityEffect,
        item::ItemEntity,
        movement::{MovementEmission, MoverType},
        persistence,
        synched_data::EntityDataAccessor,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, EntityType, properties, tags::BlockTags};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, Direction, Vec3};

pub const START_POS: EntityDataAccessor<BlockPos> = EntityDataAccessor::new(8);
#[derive(Debug)]
pub struct FallingBlockEntity {
    pub parent: EntityData,
    pub block_state: BlockState,
    pub time: i32,
    pub drop_item: bool,
    pub cancel_drop: bool,
    pub hurt_entities: bool,
    pub fall_damage_max: i32,
    pub fall_damage_per_distance: f32,
    pub block_data: Option<CompoundTag>,
    pub force_tick_after_teleport_to_duplicate: bool,
}
#[derive(Debug)]
pub enum FallingBlockLoadError {
    Base(persistence::EntityLoadError),
    Level(LevelError),
}
impl FallingBlockEntity {
    pub(crate) fn with_parent(mut parent: EntityData) -> Self {
        parent.entity_data.define(START_POS, BlockPos::ZERO);
        Self {
            parent,
            block_state: Block::SAND.default_state(),
            time: 0,
            drop_item: true,
            cancel_drop: false,
            hurt_entities: false,
            fall_damage_max: 40,
            fall_damage_per_distance: 0.0,
            block_data: None,
            force_tick_after_teleport_to_duplicate: false,
        }
    }
    pub fn new(network_id: i32, seed: i64) -> Self {
        Self::with_parent(EntityData::new(EntityType::FALLING_BLOCK, network_id, seed))
    }
    pub fn start_pos(&self) -> BlockPos {
        *self.parent.entity_data.get(START_POS)
    }
    pub fn set_start_pos(&mut self, pos: BlockPos) {
        self.parent.entity_data.set(START_POS, pos);
    }
    pub fn save_without_id(&self) -> CompoundTag {
        let mut tag = self.parent.save_without_id();
        tag.insert(
            "BlockState",
            persistence::save_block_state(self.block_state),
        );
        tag.insert("Time", Tag::Int(self.time));
        tag.insert("DropItem", Tag::Byte(i8::from(self.drop_item)));
        tag.insert("HurtEntities", Tag::Byte(i8::from(self.hurt_entities)));
        tag.insert("FallHurtAmount", Tag::Float(self.fall_damage_per_distance));
        tag.insert("FallHurtMax", Tag::Int(self.fall_damage_max));
        tag.remove("TileEntityData");
        if let Some(data) = &self.block_data {
            tag.insert("TileEntityData", Tag::Compound(data.clone()));
        }
        tag.insert("CancelDrop", Tag::Byte(i8::from(self.cancel_drop)));
        tag
    }
    pub fn load_nbt(
        &mut self,
        tag: CompoundTag,
        level: &Level,
    ) -> Result<(), FallingBlockLoadError> {
        self.block_state = tag
            .get("BlockState")
            .and_then(persistence::load_block_state)
            .unwrap_or(Block::SAND.default_state());
        self.time = tag.get_int_or("Time", 0);
        let default_hurt = level
            .block_tags()
            .map_err(FallingBlockLoadError::Level)?
            .contains_key(BlockTags::ANVIL, self.block_state.block());
        self.hurt_entities = tag.get_boolean_or("HurtEntities", default_hurt);
        self.fall_damage_per_distance = tag.get_float_or("FallHurtAmount", 0.0);
        self.fall_damage_max = tag.get_int_or("FallHurtMax", 40);
        self.drop_item = tag.get_boolean_or("DropItem", true);
        self.block_data = tag.get_compound("TileEntityData").cloned();
        self.cancel_drop = tag.get_boolean_or("CancelDrop", false);
        self.parent
            .load_nbt(tag)
            .map_err(FallingBlockLoadError::Base)
    }
}
impl EntityDataView for FallingBlockEntity {
    fn view(state: &EntityState) -> Option<&Self> {
        if let EntityState::FallingBlock(block) = state {
            Some(block)
        } else {
            None
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        if let EntityState::FallingBlock(block) = state {
            Some(block)
        } else {
            None
        }
    }
}
/// Port of FallingBlockEntity#fall. Allocation precedes synchronous source-block
/// replacement; neighbor callbacks therefore see vanilla's entity creation order.
pub fn fall(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<EntityRef, LevelError> {
    let mut entity =
        FallingBlockEntity::with_parent(level.create_entity_data(EntityType::FALLING_BLOCK)?);
    entity.block_state = state.with(properties::WATERLOGGED, false).unwrap_or(state);
    entity.parent.blocks_building = true;
    let position = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    entity.parent.set_pos(position);
    entity.parent.interpolation_old_position = position;
    entity.set_start_pos(entity.parent.block_position);
    let id = level.next_entity_ref();
    level.set_block_and_update(pos, legacy_block(state.data().fluid))?;
    level.add_entity(id, EntityState::FallingBlock(Box::new(entity)))?;
    Ok(id)
}
pub fn is_free(state: BlockState, level: &Level) -> Result<bool, LevelError> {
    Ok(state.is_air()
        || level
            .block_tags()?
            .contains_key(BlockTags::FIRE, state.block())
        || state.liquid()
        || state.data().replaceable)
}
fn drop_block_item(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let falling = level.entity::<FallingBlockEntity>(entity)?;
    let item = falling.block_state.block().as_item();
    let stack = ItemStack::new(item, 1, Default::default());
    if stack.is_empty() {
        return Ok(());
    }
    let position = falling.parent.position;
    let mut dropped = ItemEntity::new_at(level, position, stack)?;
    dropped.pickup_delay = 10;
    let id = level.next_entity_ref();
    level.add_entity(id, EntityState::Item(Box::new(dropped)))
}

pub struct FallingBlockBehavior;
impl Entity for FallingBlockBehavior {
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<crate::entity::EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }

    fn cause_fall_damage(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _distance: f64,
        _modifier: f32,
        _source: &crate::entity::DamageSource,
    ) -> Result<bool, LevelError> {
        if level.entity::<FallingBlockEntity>(entity)?.hurt_entities {
            return Err(LevelError::UnsupportedEntity(EntityType::FALLING_BLOCK));
        }
        Ok(false)
    }
    fn default_gravity(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        Ok(0.04)
    }
    fn movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    /// Concrete-powder hydration, anvils and block-entity restoration have their
    /// own block ports. Required unsupported behavior fails before ticking it.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let falling = level.entity::<FallingBlockEntity>(entity)?;
        let state = falling.block_state;
        if state.is_air() {
            return level.remove_entity(entity);
        }
        if state
            .block()
            .is_instance_of(BlockClass::ConcretePowderBlock)
            || state.block().is_instance_of(BlockClass::AnvilBlock)
        {
            return Err(LevelError::UnsupportedBlock(state.block()));
        }
        if falling.block_data.is_some() && state.has_block_entity() {
            return Err(LevelError::UnsupportedBlockEntity(state.block()));
        }
        if falling.hurt_entities {
            return Err(LevelError::UnsupportedEntity(EntityType::FALLING_BLOCK));
        }
        let time = falling.time.wrapping_add(1);
        level.entity_mut::<FallingBlockEntity>(entity)?.time = time;
        let gravity = self.effective_gravity(entity, level)?;
        if gravity != 0.0 {
            level.entity_mut::<EntityData>(entity)?.velocity.y -= gravity;
        }
        let delta = level.entity::<EntityData>(entity)?.velocity;
        self.move_by(entity, level, MoverType::SelfMovement, delta)?;
        self.apply_effects_from_blocks(entity, level)?;
        let falling = level.entity::<FallingBlockEntity>(entity)?;
        if !falling.parent.removed || falling.force_tick_after_teleport_to_duplicate {
            let pos = falling.parent.block_position;
            if !falling.parent.on_ground {
                if falling.time > 100
                    && (pos.y <= level.min_build_height() || pos.y > level.max_build_height())
                    || falling.time > 600
                {
                    if falling.drop_item && level.rules().entity_drops {
                        drop_block_item(entity, level)?;
                    }
                    level.remove_entity(entity)?;
                }
            } else {
                let current = level.get_block_state(pos)?;
                let base = level.entity_mut::<EntityData>(entity)?;
                base.velocity = base.velocity.multiply(0.7, -0.5, 0.7);
                if current.block() != Block::MOVING_PISTON {
                    if !level.entity::<FallingBlockEntity>(entity)?.cancel_drop {
                        let reader = LevelReaderRef::Server(level);
                        let empty = ItemStack::default();
                        let context = DirectionalPlaceContext::new(
                            &reader,
                            pos,
                            Direction::Down,
                            &empty,
                            Direction::Up,
                        )?;
                        let may_replace = reader.can_be_replaced(current, &context)?;
                        let continues_falling =
                            is_free(level.get_block_state(pos.below(1))?, level)?;
                        let survives = reader.can_survive(state, pos)? && !continues_falling;
                        if may_replace && survives {
                            let mut placed = state;
                            if state.get(properties::WATERLOGGED).is_some()
                                && level.get_block_state(pos)?.data().fluid.fluid_type
                                    == ferraloom_data::registries::fluid::Id::WATER
                            {
                                placed = state
                                    .with(properties::WATERLOGGED, true)
                                    .expect("present waterlogged property");
                                level.entity_mut::<FallingBlockEntity>(entity)?.block_state =
                                    placed;
                            }
                            if level.set_block_and_update(pos, placed)? {
                                let state = level.get_block_state(pos)?;
                                level.emit_entity_effect(EntityEffect::FallingBlockLanded {
                                    entity,
                                    pos,
                                    state,
                                });
                                level.remove_entity(entity)?;
                                // Sand/gravel inherit Fallable's empty onLand.
                            } else if level.entity::<FallingBlockEntity>(entity)?.drop_item
                                && level.rules().entity_drops
                            {
                                level.remove_entity(entity)?;
                                drop_block_item(entity, level)?;
                            }
                        } else {
                            level.remove_entity(entity)?;
                            if level.entity::<FallingBlockEntity>(entity)?.drop_item
                                && level.rules().entity_drops
                            {
                                drop_block_item(entity, level)?;
                            }
                        }
                    } else {
                        level.remove_entity(entity)?;
                    }
                }
            }
        }
        let air = self.air_drag(entity, level)?;
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity = base.velocity.scale(f64::from(air));
        base.requires_precise_position = base.horizontal_collision || base.vertical_collision;
        Ok(())
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _source: crate::entity::DamageSource,
        _damage: f32,
    ) -> Result<bool, LevelError> {
        if !level.entity::<EntityData>(entity)?.permanently_invulnerable {
            level.entity_mut::<EntityData>(entity)?.needs_sync = true;
        }
        Ok(false)
    }
}
