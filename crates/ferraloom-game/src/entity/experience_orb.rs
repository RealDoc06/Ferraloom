//! Port of ExperienceOrb's movement, merging, follow cache and persistence.
use super::{
    Entity, EntityData, EntityDataView, EntityRef, EntityState, collision,
    collision_context::EntityCollisionView,
    movement::{self, MovementEmission, MoverType},
    persistence,
    player::{GameType, ServerPlayer},
    synched_data::EntityDataAccessor,
};
use crate::{
    block::BlockCollisionContext,
    level::{Level, LevelError},
};
use ferraloom_data::{EntityType, tags::FluidTags};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::codec::VarInt;
use ferraloom_util::{Aabb, BlockPos, Vec3, random::RandomSource, shapes};

pub const VALUE: EntityDataAccessor<VarInt> = EntityDataAccessor::new(8);
const VALUES: [i32; 11] = [1, 3, 7, 17, 37, 73, 149, 307, 617, 1237, 2477];
#[derive(Debug)]
pub struct ExperienceOrb {
    pub parent: EntityData,
    pub age: i32,
    pub health: i32,
    pub count: i32,
    pub following_player: Option<EntityRef>,
}
impl ExperienceOrb {
    pub(crate) fn with_parent(mut parent: EntityData) -> Self {
        parent.entity_data.define(VALUE, VarInt(0));
        Self {
            parent,
            age: 0,
            health: 5,
            count: 1,
            following_player: None,
        }
    }
    pub fn new(network_id: i32, seed: i64) -> Self {
        Self::with_parent(EntityData::new(
            EntityType::EXPERIENCE_ORB,
            network_id,
            seed,
        ))
    }
    pub fn value(&self) -> i32 {
        self.parent.entity_data.get(VALUE).0
    }
    pub fn set_value(&mut self, value: i32) {
        self.parent.entity_data.set(VALUE, VarInt(value));
    }
    pub fn experience_value(max_value: i32) -> i32 {
        VALUES
            .into_iter()
            .rev()
            .find(|&v| v <= max_value)
            .unwrap_or(1)
    }
    pub fn icon(&self) -> usize {
        VALUES
            .into_iter()
            .rposition(|v| self.value() >= v)
            .unwrap_or(0)
    }
    pub fn save_without_id(&self) -> CompoundTag {
        let mut tag = self.parent.save_without_id();
        tag.insert("Health", Tag::Short(self.health as i16));
        tag.insert("Age", Tag::Short(self.age as i16));
        tag.insert("Value", Tag::Short(self.value() as i16));
        tag.insert("Count", Tag::Int(self.count));
        tag
    }
    pub fn load_nbt(&mut self, tag: CompoundTag) -> Result<(), persistence::EntityLoadError> {
        self.health = i32::from(tag.get_short_or("Health", 5));
        self.age = i32::from(tag.get_short_or("Age", 0));
        self.set_value(i32::from(tag.get_short_or("Value", 0)));
        self.count = tag.get_int("Count").filter(|&v| v > 0).unwrap_or(1);
        self.parent.load_nbt(tag)
    }
}
impl EntityDataView for ExperienceOrb {
    fn view(state: &EntityState) -> Option<&Self> {
        if let EntityState::ExperienceOrb(orb) = state {
            Some(orb)
        } else {
            None
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        if let EntityState::ExperienceOrb(orb) = state {
            Some(orb)
        } else {
            None
        }
    }
}
/// Port of ExperienceOrb's server constructor, including its before-add collision
/// queries. The context captures the unregistered nonliving entity's immutable type.
pub fn spawn(
    level: &mut Level,
    position: Vec3,
    roughly: Vec3,
    value: i32,
) -> Result<EntityRef, LevelError> {
    let id = level.next_entity_ref();
    let mut orb = ExperienceOrb::with_parent(level.create_entity_data(EntityType::EXPERIENCE_ORB)?);
    orb.parent.set_pos(position);
    orb.parent.yaw = orb.parent.random.next_float() * 360.0;
    let mut velocity = Vec3::new(
        (orb.parent.random.next_double() * 0.2 - 0.1) * 2.0,
        orb.parent.random.next_double() * 0.2 * 2.0,
        (orb.parent.random.next_double() * 0.2 - 0.1) * 2.0,
    );
    if roughly.length_sqr() > 0.0 && roughly.dot(velocity) < 0.0 {
        velocity = velocity.scale(-1.0);
    }
    let size = orb.parent.bounding_box.size();
    orb.parent
        .set_pos(position + roughly.normalize().scale(size * 0.5));
    orb.parent.velocity = velocity;
    if !level.no_collision(None, orb.parent.bounding_box, false)? {
        let height = f64::from(orb.parent.dimensions.height);
        let width = f64::from(orb.parent.dimensions.width);
        let center = orb.parent.position.add_xyz(0.0, height / 2.0, 0.0);
        let allowed = shapes::create_aabb(Aabb::of_size(center, size, size, size));
        let context = BlockCollisionContext::Entity(
            EntityCollisionView::capture_untracked_nonliving(id, &orb.parent),
        );
        if let Some(position) = collision::find_free_position_from_context(
            level,
            &context,
            &allowed,
            center,
            Vec3::new(width, height, width),
        )? {
            orb.parent
                .set_pos(position.subtract_xyz(0.0, height / 2.0, 0.0));
        }
    }
    orb.set_value(value);
    level.add_entity(id, EntityState::ExperienceOrb(Box::new(orb)))?;
    Ok(id)
}
pub fn award(
    level: &mut Level,
    position: Vec3,
    roughly: Vec3,
    mut amount: i32,
) -> Result<(), LevelError> {
    while amount > 0 {
        let value = ExperienceOrb::experience_value(amount);
        amount -= value;
        let group = level.random().next_int_bound(40);
        let candidates = level.entities_of_class::<ExperienceOrb>(
            Aabb::of_size(position, 1.0, 1.0, 1.0),
            |_, orb| {
                !orb.parent.removed
                    && orb.parent.network_id.wrapping_sub(group) % 40 == 0
                    && orb.value() == value
            },
        );
        if let Some(&id) = candidates.first() {
            let orb = level.entity_mut::<ExperienceOrb>(id)?;
            orb.count = orb.count.wrapping_add(1);
            orb.age = 0;
        } else {
            spawn(level, position, roughly, value)?;
        }
    }
    Ok(())
}
fn scan_for_merges(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let orb = level.entity::<ExperienceOrb>(entity)?;
    let (network, value) = (orb.parent.network_id, orb.value());
    let candidates = level.entities_of_class::<ExperienceOrb>(
        orb.parent.bounding_box.inflate(0.5, 0.5, 0.5),
        |id, other| {
            id != entity
                && !other.parent.removed
                && other.parent.network_id.wrapping_sub(network) % 40 == 0
                && other.value() == value
        },
    );
    for other in candidates {
        let other_orb = level.entity::<ExperienceOrb>(other)?;
        let (count, age) = (other_orb.count, other_orb.age);
        let orb = level.entity_mut::<ExperienceOrb>(entity)?;
        orb.count = orb.count.wrapping_add(count);
        orb.age = orb.age.min(age);
        level.remove_entity(other)?;
    }
    Ok(())
}
/// Existing cached targets intentionally keep their last position after removal.
/// Only spectator status and distance invalidate the cache in vanilla.
pub(crate) fn follow_nearby_player(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let orb = level.entity::<ExperienceOrb>(entity)?;
    let position = orb.parent.position;
    let current = orb.following_player;
    let invalid = if let Some(id) = current {
        let p = level.entity::<ServerPlayer>(id)?;
        p.game_mode == GameType::Spectator || p.position.distance_to_sqr(position) > 64.0
    } else {
        true
    };
    let target = if invalid {
        let mut best = 64.0;
        let mut nearest = None;
        // Always-ticking player lookup preserves their level insertion order.
        for id in level.all_entities() {
            if let Ok(player) = level.entity::<ServerPlayer>(id)
                && player.game_mode != GameType::Spectator
            {
                let distance = player.position.distance_to_sqr(position);
                if distance < best {
                    best = distance;
                    nearest = Some(id);
                }
            }
        }
        nearest.filter(|id| {
            level
                .entity::<ServerPlayer>(*id)
                .is_ok_and(|p| p.parent.health() > 0.0)
        })
    } else {
        current
    };
    level.entity_mut::<ExperienceOrb>(entity)?.following_player = target;
    if let Some(player) = target {
        let player = level.entity::<ServerPlayer>(player)?;
        let delta = Vec3::new(
            player.position.x - position.x,
            player.position.y + f64::from(player.eye_height) / 2.0 - position.y,
            player.position.z - position.z,
        );
        let power = 1.0 - delta.length_sqr().sqrt() / 8.0;
        let orb = level.entity_mut::<ExperienceOrb>(entity)?;
        orb.parent.velocity = orb.parent.velocity + delta.normalize().scale(power * power * 0.1);
    }
    Ok(())
}
pub struct ExperienceOrbBehavior;
impl Entity for ExperienceOrbBehavior {
    fn player_touch(
        &self,
        _entity: EntityRef,
        _player: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.unported_operation(crate::block::UnportedOperation::EntityMethod(
            EntityType::EXPERIENCE_ORB,
            super::EntityMethod::PlayerTouch,
        ))
    }
    fn do_water_splash_effect(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }

    fn sound_source(&self) -> ferraloom_protocol::play::SoundSource {
        ferraloom_protocol::play::SoundSource::Ambient
    }

    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        super::trace_base_entity_refs(state.as_entity(), references);
        if let Some(target) = ExperienceOrb::view(state)
            .expect("orb dispatcher state")
            .following_player
        {
            references.push(target);
        }
    }

    fn default_gravity(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        Ok(0.03)
    }
    fn movement_emission(&self) -> MovementEmission {
        MovementEmission::None
    }
    fn block_pos_below_that_affects_my_movement(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<BlockPos, LevelError> {
        movement::get_on_pos(entity, 0.999999_f32, level)
    }
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        super::base_tick(self, entity, level)?;
        let bounds = level.entity::<EntityData>(entity)?.bounding_box;
        let colliding = !level.no_collision(None, bounds, false)?;
        if level
            .entity::<EntityData>(entity)?
            .fluid_interaction
            .eyes_inside(FluidTags::WATER, level)?
        {
            let base = level.entity_mut::<EntityData>(entity)?;
            let v = base.velocity;
            base.velocity = Vec3::new(
                v.x * 0.9900000095367432,
                ferraloom_util::java::min(v.y + 5.000000237487257e-4, 0.05999999865889549),
                v.z * 0.9900000095367432,
            );
        } else if !colliding {
            let gravity = self.effective_gravity(entity, level)?;
            if gravity != 0.0 {
                level.entity_mut::<EntityData>(entity)?.velocity.y -= gravity;
            }
        }
        let pos = level.entity::<EntityData>(entity)?.block_position;
        let fluid = level.get_block_state(pos)?.data().fluid;
        if !fluid.empty
            && level
                .fluid_tags()?
                .contains_key(FluidTags::LAVA, fluid.fluid_type)
        {
            let base = level.entity_mut::<EntityData>(entity)?;
            base.velocity = Vec3::new(
                f64::from((base.random.next_float() - base.random.next_float()) * 0.2),
                0.20000000298023224,
                f64::from((base.random.next_float() - base.random.next_float()) * 0.2),
            );
        }
        if level.entity::<EntityData>(entity)?.tick_count % 20 == 1 {
            scan_for_merges(entity, level)?;
        }
        follow_nearby_player(entity, level)?;
        let base = level.entity::<ExperienceOrb>(entity)?;
        if base.following_player.is_none()
            && colliding
            && !level.no_collision(
                None,
                base.parent.bounding_box.move_vec(base.parent.velocity),
                false,
            )?
        {
            super::item::item_entity::move_towards_closest_space(entity, level)?;
            level.entity_mut::<EntityData>(entity)?.needs_sync = true;
        }
        let velocity = level.entity::<EntityData>(entity)?.velocity;
        self.move_by(entity, level, MoverType::SelfMovement, velocity)?;
        self.apply_effects_from_blocks(entity, level)?;
        let on_ground = level.entity::<EntityData>(entity)?.on_ground;
        let mut friction = self.air_drag(entity, level)?;
        if on_ground {
            friction *= level
                .get_block_state(self.block_pos_below_that_affects_my_movement(entity, level)?)?
                .block()
                .data()
                .friction;
        }
        let gravity = self.effective_gravity(entity, level)?;
        let orb = level.entity_mut::<ExperienceOrb>(entity)?;
        orb.parent.velocity = orb.parent.velocity.scale(f64::from(friction));
        if orb.parent.vertical_collision_below && velocity.y < -gravity {
            orb.parent.velocity.y = -velocity.y * 0.4;
        }
        orb.age = orb.age.wrapping_add(1);
        if orb.age >= 6000 {
            level.remove_entity(entity)?;
        }
        Ok(())
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _source: super::DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let orb = level.entity_mut::<ExperienceOrb>(entity)?;
        if orb.parent.permanently_invulnerable || orb.parent.removed {
            return Ok(false);
        }
        orb.parent.mark_hurt();
        orb.health = (orb.health as f32 - damage) as i32;
        if orb.health <= 0 {
            level.remove_entity(entity)?;
        }
        Ok(true)
    }
}
