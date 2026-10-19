//! Canonical Entity state, embedded exactly once in every concrete entity.
use std::{cell::Cell, collections::VecDeque};

use ferraloom_data::{BlockState, EntityType};
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::{codec::VarInt, entity_data::Pose};
use ferraloom_util::{
    Aabb, BlockPos, ChunkPos, Uuid, Vec3, collections::JavaHashSet, mth, random::LegacyRandomSource,
};

use super::{
    EntityRef,
    synched_data::{self, SynchedEntityData},
};

pub use super::dimensions::EntityDimensions;

/// Port of Entity.RemovalReason, including chunk unload's persistence semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemovalReason {
    Killed,
    Discarded,
    UnloadedToChunk,
    UnloadedWithPlayer,
    ChangedDimension,
}
impl RemovalReason {
    pub fn should_destroy(self) -> bool {
        matches!(self, Self::Killed | Self::Discarded)
    }
    pub fn should_save(self) -> bool {
        self == Self::UnloadedToChunk
    }
}

/// Port of Entity.Movement; None denotes a combined/direct movement path.
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub from: Vec3,
    pub to: Vec3,
    pub axis_dependent_original_movement: Option<Vec3>,
}

/// Port of Entity's mutable base state. World ownership and callbacks use handles.
#[derive(Debug)]
pub struct EntityData {
    pub entity_type: EntityType,
    pub network_id: i32,
    pub(crate) network_id_allocated: bool,
    pub uuid: Uuid,
    pub requires_precise_position: bool,
    pub blocks_building: bool,
    pub passengers: super::PassengerList,
    pub boarding_cooldown: i32,
    pub vehicle: Cell<Option<EntityRef>>,
    pub position: Vec3,
    pub block_position: BlockPos,
    pub chunk_position: ChunkPos,
    pub velocity: Vec3,
    pub old_position: Vec3,
    pub interpolation_old_position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub old_yaw: f32,
    pub old_pitch: f32,
    pub bounding_box: Aabb,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub vertical_collision_below: bool,
    pub minor_horizontal_collision: bool,
    pub stuck_speed_multiplier: Vec3,
    pub removed: bool,
    pub removal_reason: Option<RemovalReason>,
    pub move_dist: f32,
    pub fly_dist: f32,
    pub fall_distance: f64,
    pub next_step: f32,
    pub no_physics: bool,
    pub random: LegacyRandomSource,
    pub tick_count: i32,
    pub remaining_fire_ticks: i32,
    pub was_touching_water: bool,
    pub was_eye_in_water: bool,
    pub fluid_interaction: super::fluid::EntityFluidInteraction,
    pub invulnerable_time: i32,
    pub first_tick: bool,
    pub entity_data: SynchedEntityData,
    pub packet_position: Vec3,
    pub needs_sync: bool,
    pub sync_position: bool,
    pub sync_velocity: bool,
    pub portal_cooldown: i32,
    pub permanently_invulnerable: bool,
    pub has_glowing_tag: bool,
    pub tags: JavaHashSet<String>,
    pub piston_deltas: [f64; 3],
    pub piston_deltas_game_time: i64,
    pub dimensions: EntityDimensions,
    pub eye_height: f32,
    pub is_in_powder_snow: bool,
    pub was_in_powder_snow: bool,
    pub supporting_block_pos: Option<BlockPos>,
    pub on_ground_no_blocks: bool,
    pub crystal_sound_intensity: f32,
    pub last_crystal_sound_play_tick: i32,
    pub has_visual_fire: bool,
    pub last_known_speed: Vec3,
    pub last_known_position: Option<Vec3>,
    pub in_block_state: Cell<Option<BlockState>>,
    pub movement_this_tick: VecDeque<Movement>,
    pub final_movements_this_tick: Vec<Movement>,
    pub inside_effect_collector: super::inside_block::StepBasedCollector,
    pub visited_blocks: ferraloom_util::collections::LongOpenHashSet,
    pub inside_block_is_precise: bool,
    pub custom_data: CompoundTag,
    /// Unknown and subclass NBT is retained; known fields are regenerated on save.
    pub saved: CompoundTag,
}
impl Default for EntityData {
    fn default() -> Self {
        Self::from_random(
            EntityType::COW,
            0,
            crate::level::entity::new_entity_random(),
        )
    }
}
impl EntityData {
    /// RandomSource.create's seed is supplied by the owning server RNG allocator.
    pub fn new(kind: EntityType, network_id: i32, seed: i64) -> Self {
        Self::from_random(kind, network_id, LegacyRandomSource::new(seed))
    }
    pub(crate) fn from_random(
        kind: EntityType,
        network_id: i32,
        random: LegacyRandomSource,
    ) -> Self {
        Self::from_random_with_id(kind, random, || network_id)
    }
    /// Entity's random and UUID field initializers run before the constructor's
    /// Level#getNextEntityId call; subclass RNG draws follow this base creation.
    pub(crate) fn from_random_with_id(
        kind: EntityType,
        mut random: LegacyRandomSource,
        next_id: impl FnOnce() -> i32,
    ) -> Self {
        let uuid = Uuid::insecure(&mut random);
        let network_id = next_id();
        let dimensions = EntityDimensions::for_type(kind);
        Self {
            entity_type: kind,
            network_id,
            network_id_allocated: network_id != 0,
            uuid,
            requires_precise_position: false,
            blocks_building: false,
            passengers: super::PassengerList::default(),
            boarding_cooldown: 0,
            vehicle: Cell::new(None),
            position: Vec3::ZERO,
            block_position: BlockPos::ZERO,
            chunk_position: ChunkPos::new(0, 0),
            velocity: Vec3::ZERO,
            old_position: Vec3::ZERO,
            interpolation_old_position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            old_yaw: 0.0,
            old_pitch: 0.0,
            bounding_box: dimensions.make_bounding_box(Vec3::ZERO),
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            vertical_collision_below: false,
            minor_horizontal_collision: false,
            stuck_speed_multiplier: Vec3::ZERO,
            removed: false,
            removal_reason: None,
            move_dist: 0.0,
            fly_dist: 0.0,
            fall_distance: 0.0,
            next_step: 1.0,
            no_physics: false,
            random,
            tick_count: 0,
            remaining_fire_ticks: 0,
            was_touching_water: false,
            was_eye_in_water: false,
            fluid_interaction: super::fluid::EntityFluidInteraction::default(),
            invulnerable_time: 0,
            first_tick: true,
            entity_data: SynchedEntityData::default(),
            packet_position: Vec3::ZERO,
            needs_sync: false,
            sync_position: false,
            sync_velocity: false,
            portal_cooldown: 0,
            permanently_invulnerable: false,
            has_glowing_tag: false,
            tags: JavaHashSet::new(),
            piston_deltas: [0.0; 3],
            piston_deltas_game_time: 0,
            eye_height: dimensions.eye_height,
            dimensions,
            is_in_powder_snow: false,
            was_in_powder_snow: false,
            supporting_block_pos: None,
            on_ground_no_blocks: false,
            crystal_sound_intensity: 0.0,
            last_crystal_sound_play_tick: 0,
            has_visual_fire: false,
            last_known_speed: Vec3::ZERO,
            last_known_position: None,
            in_block_state: Cell::new(None),
            movement_this_tick: VecDeque::with_capacity(100),
            final_movements_this_tick: Vec::new(),
            inside_effect_collector: super::inside_block::StepBasedCollector::default(),
            visited_blocks: ferraloom_util::collections::LongOpenHashSet::new(),
            inside_block_is_precise: false,
            custom_data: CompoundTag::new(),
            saved: CompoundTag::new(),
        }
    }
    pub fn set_pos_raw(&mut self, pos: Vec3) {
        if self.position.x != pos.x || self.position.y != pos.y || self.position.z != pos.z {
            self.position = pos;
            let block = BlockPos::new(mth::floor(pos.x), mth::floor(pos.y), mth::floor(pos.z));
            if block != self.block_position {
                self.block_position = block;
                self.in_block_state.set(None);
                self.chunk_position = ChunkPos::containing(block);
            }
        }
    }
    /// Port of Entity#setPos; the Level facade also updates section membership.
    pub fn set_pos(&mut self, pos: Vec3) {
        self.set_pos_raw(pos);
        self.bounding_box = self.dimensions.make_bounding_box(self.position);
    }
    /// Port of Entity#snapTo before registration. Registered entities use Level's
    /// handle operations so section membership follows the accepted position.
    pub fn snap_to(&mut self, position: Vec3, yaw: f32, pitch: f32) {
        self.set_pos_raw(position);
        self.set_y_rot(yaw);
        self.set_x_rot(pitch);
        self.set_old_pos_and_rot();
        self.last_known_position = None;
        self.set_pos(self.position);
    }
    pub fn set_bounding_box(&mut self, bb: Aabb) {
        self.bounding_box = bb;
    }
    /// Port of Entity#getInBlockState. The cache belongs to this entity and is
    /// invalidated on block-position changes and at the native base-tick point.
    pub fn in_block_state(
        &self,
        level: &crate::level::Level,
    ) -> Result<BlockState, crate::level::LevelError> {
        if let Some(state) = self.in_block_state.get() {
            return Ok(state);
        }
        let state = level.get_block_state(self.block_position)?;
        self.in_block_state.set(Some(state));
        Ok(state)
    }
    /// Port of Entity#moveRelative. Input acceleration does not mark needsSync.
    pub fn move_relative(&mut self, speed: f32, input: Vec3) {
        self.velocity = self.velocity + super::movement::input_vector(input, speed, self.yaw);
    }
    /// Port of Entity#setDeltaMovement; reject the entire non-finite vector.
    pub fn set_delta_movement(&mut self, movement: Vec3) {
        if movement.is_finite() {
            self.velocity = movement;
        }
    }
    /// Port of Entity#push(double,double,double), including finite checks and
    /// needsSync even when the accepted impulse is zero.
    pub fn push_impulse(&mut self, impulse: Vec3) {
        if impulse.is_finite() {
            self.velocity = self.velocity + impulse;
            self.needs_sync = true;
        }
    }
    /// Port of Entity#getLookAngle using its current, uninterpolated rotation.
    pub fn look_angle(&self) -> Vec3 {
        super::movement::calculate_view_vector(self.pitch, self.yaw)
    }
    /// Port of Entity#setYRot. Non-finite input leaves the previous angle intact.
    pub fn set_y_rot(&mut self, yaw: f32) {
        if yaw.is_finite() {
            self.yaw = yaw;
        }
    }
    /// Port of Entity#setXRot, including the remainder before the pitch clamp.
    pub fn set_x_rot(&mut self, pitch: f32) {
        if pitch.is_finite() {
            self.pitch = (pitch % 360.0).clamp(-90.0, 90.0);
        }
    }
    /// Port of Entity#absSnapRotationTo, preserving position history.
    pub fn abs_snap_rotation_to(&mut self, yaw: f32, pitch: f32) {
        self.set_y_rot(yaw % 360.0);
        self.set_x_rot(pitch.clamp(-90.0, 90.0) % 360.0);
        self.old_yaw = self.yaw;
        self.old_pitch = self.pitch;
    }
    /// Port of Entity#removeLatestMovementRecording; an empty queue is unchanged.
    pub fn remove_latest_movement_recording(&mut self) {
        self.movement_this_tick.pop_back();
    }
    pub fn set_old_pos_and_rot(&mut self) {
        self.old_position = self.position;
        self.interpolation_old_position = self.position;
        self.old_yaw = self.yaw;
        self.old_pitch = self.pitch;
    }
    pub fn shared_flag(&self, bit: u8) -> bool {
        *self.entity_data.get(synched_data::SHARED_FLAGS) as u8 & (1 << bit) != 0
    }
    /// Parent body of Entity#isSwimming; Player adds its flight/spectator gates.
    pub fn is_swimming(&self) -> bool {
        self.shared_flag(4)
    }
    pub fn set_shared_flag(&mut self, bit: u8, enabled: bool) {
        let flags = *self.entity_data.get(synched_data::SHARED_FLAGS) as u8;
        self.entity_data.set(
            synched_data::SHARED_FLAGS,
            (if enabled {
                flags | (1 << bit)
            } else {
                flags & !(1 << bit)
            }) as i8,
        );
    }
    pub fn ticks_frozen(&self) -> i32 {
        self.entity_data.get(synched_data::TICKS_FROZEN).0
    }
    pub fn set_ticks_frozen(&mut self, ticks: i32) {
        self.entity_data
            .set(synched_data::TICKS_FROZEN, VarInt(ticks));
    }
    pub const fn ticks_required_to_freeze(&self) -> i32 {
        140
    }
    pub fn percent_frozen(&self) -> f32 {
        self.ticks_frozen().min(self.ticks_required_to_freeze()) as f32
            / self.ticks_required_to_freeze() as f32
    }
    pub fn is_fully_frozen(&self) -> bool {
        self.ticks_frozen() >= self.ticks_required_to_freeze()
    }
    /// Port of Entity#isInLava: the first base tick is deliberately excluded.
    pub fn is_in_lava(
        &self,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        Ok(!self.first_tick
            && self
                .fluid_interaction
                .height(ferraloom_data::tags::FluidTags::LAVA, level)?
                > 0.0)
    }
    /// Port of Entity#getFluidJumpThreshold; the eye-height float is widened
    /// before comparing it with the native double literal.
    pub fn fluid_jump_threshold(&self) -> f64 {
        if f64::from(self.eye_height) < 0.4 {
            0.0
        } else {
            0.4
        }
    }
    pub fn clear_freeze(&mut self) {
        self.entity_data.set(synched_data::TICKS_FROZEN, VarInt(0));
    }
    pub fn clear_fire(&mut self) {
        self.remaining_fire_ticks = self.remaining_fire_ticks.min(0);
    }
    /// Port of Entity#markHurt. This requests the forced velocity packet sent to
    /// tracking players and self, independently of the normal interval gate.
    pub fn mark_hurt(&mut self) {
        self.sync_velocity = true;
    }
    pub fn ignite_for_ticks(&mut self, ticks: i32) {
        self.remaining_fire_ticks = self.remaining_fire_ticks.max(ticks);
        self.clear_freeze();
    }
    pub fn add_movement(&mut self, movement: Movement) {
        if self.movement_this_tick.len() >= 100 {
            let first = self.movement_this_tick.pop_front().unwrap();
            let second = self.movement_this_tick.pop_front().unwrap();
            self.movement_this_tick.push_front(Movement {
                from: first.from,
                to: second.to,
                axis_dependent_original_movement: None,
            });
        }
        self.movement_this_tick.push_back(movement);
    }
    /// Port of Entity#checkFallDamage's accumulation before the fallOn callback.
    pub fn accumulate_fall_distance(&mut self, dy: f64) {
        if !self.was_touching_water && dy < 0.0 {
            self.fall_distance -= f64::from(dy as f32);
        }
    }
    pub fn set_removed(&mut self, reason: RemovalReason) {
        if self.removal_reason.is_none() {
            self.removal_reason = Some(reason);
        }
        self.removed = true;
    }
    pub fn pose(&self) -> Pose {
        *self.entity_data.get(synched_data::POSE)
    }
}
