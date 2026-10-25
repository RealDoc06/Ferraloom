//! Port of FallingBlockEntity's physics, water interception, landing and persistence.
use crate::{
    block::{BlockMethod, DirectionalPlaceContext, LevelReaderRef, fluid::legacy_block},
    entity::{
        Entity, EntityData, EntityDataView, EntityRef, EntityState,
        item::ItemEntity,
        movement::{MovementEmission, MoverType},
        persistence,
        synched_data::EntityDataAccessor,
    },
    level::{
        Level, LevelError,
        clip::{ClipBlock, ClipContext, ClipFluid},
    },
};
use ferraloom_data::{
    Block, BlockClass, BlockState, EntityType, properties,
    tags::{BlockTags, FluidTags},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, Direction, Vec3, mth, random::RandomSource};

#[cfg(test)]
mod powder_tests;
#[cfg(test)]
mod tests;

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
        self.parent
            .load_nbt(tag.clone())
            .map_err(FallingBlockLoadError::Base)?;
        self.read_block_state(&tag);
        let default_hurt = level
            .block_tags()
            .map_err(FallingBlockLoadError::Level)?
            .contains_key(BlockTags::ANVIL, self.block_state.block());
        self.read_additional_save_data(&tag, default_hurt);
        Ok(())
    }
    pub(crate) fn read_block_state(&mut self, tag: &CompoundTag) {
        self.block_state = tag
            .get("BlockState")
            .and_then(persistence::load_block_state)
            .unwrap_or(Block::SAND.default_state());
    }
    pub(crate) fn read_additional_save_data(&mut self, tag: &CompoundTag, default_hurt: bool) {
        self.time = tag.get_int_or("Time", 0);
        self.hurt_entities = tag.get_boolean_or("HurtEntities", default_hurt);
        self.fall_damage_per_distance = tag.get_float_or("FallHurtAmount", 0.0);
        self.fall_damage_max = tag.get_int_or("FallHurtMax", 40);
        self.drop_item = tag.get_boolean_or("DropItem", true);
        self.block_data = tag.get_compound("TileEntityData").cloned();
        self.cancel_drop = tag.get_boolean_or("CancelDrop", false);
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
        FallingBlockEntity::with_parent(level.create_entity_data(EntityType::FALLING_BLOCK));
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
fn drop_block_item(entity: EntityRef, block: Block, level: &mut Level) -> Result<(), LevelError> {
    let falling = level.entity::<FallingBlockEntity>(entity)?;
    let item = block.as_item();
    let stack = ItemStack::new(item, 1, Default::default());
    if stack.is_empty() {
        return Ok(());
    }
    let position = falling.parent.position;
    let mut dropped = ItemEntity::new_at(level, position, stack);
    dropped.pickup_delay = 10;
    let id = level.next_entity_ref();
    level.add_entity(id, EntityState::Item(Box::new(dropped)))
}

/// Native callback frames and the getEntities result hold entity objects even
/// when a callback removes them and runs management before the loop resumes.
fn with_entity_refs<T>(
    references: &[EntityRef],
    level: &mut Level,
    body: impl FnOnce(&mut Level) -> Result<T, LevelError>,
) -> Result<T, LevelError> {
    for (retained, &entity) in references.iter().enumerate() {
        if let Err(error) = level.retain_entity(entity) {
            for &prior in references[..retained].iter().rev() {
                level.release_entity(prior);
            }
            return Err(error);
        }
    }
    let result = body(level);
    for &entity in references.iter().rev() {
        level.release_entity(entity);
    }
    result
}

/// Port of FallingBlockEntity#causeFallDamage. The block owns the damage source;
/// victims are captured in native query order before any synchronous hurt call.
fn cause_fall_damage(
    entity: EntityRef,
    distance: f64,
    level: &mut Level,
) -> Result<bool, LevelError> {
    if !level.entity::<FallingBlockEntity>(entity)?.hurt_entities {
        return Ok(false);
    }
    let distance = mth::ceil(distance - 1.0);
    if distance < 0 {
        return Ok(false);
    }
    let block = level
        .entity::<FallingBlockEntity>(entity)?
        .block_state
        .block();
    let source = level
        .required_block_behavior(block, BlockMethod::GetFallDamageSource)?
        .get_fall_damage_source(entity, level)?;
    let falling = level.entity::<FallingBlockEntity>(entity)?;
    let damage = mth::floor(f64::from(
        distance as f32 * falling.fall_damage_per_distance,
    ))
    .min(falling.fall_damage_max) as f32;
    let bounds = falling.parent.bounding_box;
    let mut victims = Vec::new();
    for victim in level.entities_in(bounds, |id, _| id != entity) {
        let behavior = level.entity_behavior(victim)?;
        if let EntityState::Player(player) = level.entity_state(victim)?
            && (behavior.is_spectator(victim, level)?
                || player.game_mode == crate::entity::player::GameType::Creative)
        {
            continue;
        }
        if behavior.is_alive(victim, level)? && level.entity_state(victim)?.as_living().is_some() {
            victims.push(victim);
        }
    }
    let mut references = vec![entity];
    references.extend_from_slice(&victims);
    source.trace_entity_refs(&mut references);
    with_entity_refs(&references, level, |level| {
        for victim in victims {
            level
                .entity_behavior(victim)?
                .hurt_server(victim, level, source.clone(), damage)?;
        }
        // Hurt callbacks may have changed the canonical block being carried.
        let state = level.entity::<FallingBlockEntity>(entity)?.block_state;
        let is_anvil = level
            .block_tags()?
            .contains_key(BlockTags::ANVIL, state.block());
        if is_anvil
            && damage > 0.0
            && level.entity_mut::<EntityData>(entity)?.random.next_float()
                < 0.05_f32 + distance as f32 * 0.05_f32
        {
            let state = level.entity::<FallingBlockEntity>(entity)?.block_state;
            let next = crate::block::building::anvil::damage(state);
            let falling = level.entity_mut::<FallingBlockEntity>(entity)?;
            match next {
                Some(state) => falling.block_state = state,
                None => falling.cancel_drop = true,
            }
        }
        Ok(false)
    })
}

/// Port of FallingBlockEntity#callOnBrokenAfterFall, using tick's original Block
/// receiver even when movement degraded the canonical state in the meantime.
fn call_on_broken_after_fall(
    entity: EntityRef,
    block: Block,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .required_block_behavior(block, BlockMethod::OnBrokenAfterFall)?
        .on_broken_after_fall(pos, entity, level)
}

fn send_landing_packets(
    entity: EntityRef,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    use crate::entity::tracking::{TrackingAudience, TrackingPacket};
    use ferraloom_protocol::play::clientbound as cb;
    let state = level.get_block_state(pos)?;
    level
        .broadcast_tracked_packet(
            entity,
            TrackingPacket {
                audience: TrackingAudience::Players,
                packet: cb::Packet::BlockUpdate(cb::BlockUpdate {
                    pos,
                    block_state: ferraloom_protocol::play::BlockStateId(i32::from(state.id())),
                }),
            },
        )
        .map_err(|error| error.into_level(level))?;
    let state = level.get_block_state(pos)?;
    level
        .broadcast_tracked_packet(
            entity,
            TrackingPacket {
                audience: TrackingAudience::Players,
                packet: cb::Packet::AddTransientBlock(cb::AddTransientBlock {
                    pos,
                    block_state: ferraloom_protocol::play::BlockStateId(i32::from(state.id())),
                }),
            },
        )
        .map_err(|error| error.into_level(level))
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
        distance: f64,
        _modifier: f32,
        _source: &crate::entity::DamageSource,
    ) -> Result<bool, LevelError> {
        cause_fall_damage(entity, distance, level)
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
    /// Port of FallingBlockEntity#tick. Block-entity restoration remains required.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        with_entity_refs(&[entity], level, |level| {
            self.tick_falling_block(entity, level)
        })
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
impl FallingBlockBehavior {
    fn tick_falling_block(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let falling = level.entity::<FallingBlockEntity>(entity)?;
        let state = falling.block_state;
        let block = state.block();
        if state.is_air() {
            return level.remove_entity(entity);
        }
        if falling.block_data.is_some() && state.has_block_entity() {
            return Err(LevelError::UnsupportedBlockEntity(state.block()));
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
            let mut pos = falling.parent.block_position;
            let is_concrete = falling
                .block_state
                .block()
                .is_instance_of(BlockClass::ConcretePowderBlock);
            let mut in_water = if is_concrete {
                let fluid = level.get_block_state(pos)?.data().fluid;
                level
                    .fluid_tags()?
                    .contains_key(FluidTags::WATER, fluid.fluid_type)
            } else {
                false
            };
            if is_concrete && falling.parent.velocity.length_sqr() > 1.0 {
                let context = ClipContext::for_entity(
                    falling.parent.old_position,
                    falling.parent.position,
                    ClipBlock::Collider,
                    ClipFluid::SourceOnly,
                    Some(entity),
                    level,
                )?;
                let hit = level.clip(&context)?;
                if !hit.miss && {
                    let fluid = level.get_block_state(hit.block_pos)?.data().fluid;
                    level
                        .fluid_tags()?
                        .contains_key(FluidTags::WATER, fluid.fluid_type)
                } {
                    pos = hit.block_pos;
                    in_water = true;
                }
            }
            if !falling.parent.on_ground && !in_water {
                if falling.time > 100
                    && (pos.y <= level.min_build_height() || pos.y > level.max_build_height())
                    || falling.time > 600
                {
                    if falling.drop_item && level.rules().entity_drops {
                        drop_block_item(entity, block, level)?;
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
                            is_free(level.get_block_state(pos.below(1))?, level)?
                                && (!is_concrete || !in_water);
                        let state = level.entity::<FallingBlockEntity>(entity)?.block_state;
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
                                send_landing_packets(entity, pos, level)?;
                                level.remove_entity(entity)?;
                                let state = level.entity::<FallingBlockEntity>(entity)?.block_state;
                                level
                                    .required_block_behavior(block, BlockMethod::OnLand)?
                                    .on_land(state, pos, current, entity, level)?;
                            } else if level.entity::<FallingBlockEntity>(entity)?.drop_item
                                && level.rules().entity_drops
                            {
                                level.remove_entity(entity)?;
                                call_on_broken_after_fall(entity, block, pos, level)?;
                                drop_block_item(entity, block, level)?;
                            }
                        } else {
                            level.remove_entity(entity)?;
                            if level.entity::<FallingBlockEntity>(entity)?.drop_item
                                && level.rules().entity_drops
                            {
                                call_on_broken_after_fall(entity, block, pos, level)?;
                                drop_block_item(entity, block, level)?;
                            }
                        }
                    } else {
                        level.remove_entity(entity)?;
                        call_on_broken_after_fall(entity, block, pos, level)?;
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
}
