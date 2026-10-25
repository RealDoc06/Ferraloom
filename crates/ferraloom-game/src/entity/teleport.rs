//! Entity position changes and the canonical player listener teleport path.
use super::{EntityData, EntityRef, player::ServerPlayer};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_protocol::{
    codec::VarInt,
    play::{PositionMoveRotation, clientbound as cb},
};
use ferraloom_util::Vec3;

/// Port of Relative's nine protocol flags. Unknown wire bits are ignored by unpack.
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Relative {
    X,
    Y,
    Z,
    YRot,
    XRot,
    DeltaX,
    DeltaY,
    DeltaZ,
    RotateDelta,
}
impl Relative {
    pub const ALL: i32 = (1 << 9) - 1;
    pub const ROTATION: i32 = Self::YRot.mask() | Self::XRot.mask();
    pub const DELTA: i32 =
        Self::DeltaX.mask() | Self::DeltaY.mask() | Self::DeltaZ.mask() | Self::RotateDelta.mask();
    pub const fn mask(self) -> i32 {
        1 << self as u8
    }
}

/// Port of PositionMoveRotation#calculateAbsolute. Angle arithmetic is f32;
/// toRadians runs in double precision before Vec3's float trigonometry.
pub fn calculate_absolute(
    source: PositionMoveRotation,
    change: PositionMoveRotation,
    relatives: i32,
) -> PositionMoveRotation {
    let has = |flag: Relative| relatives & flag.mask() != 0;
    let position = Vec3::new(
        (if has(Relative::X) {
            source.position.x
        } else {
            0.0
        }) + change.position.x,
        (if has(Relative::Y) {
            source.position.y
        } else {
            0.0
        }) + change.position.y,
        (if has(Relative::Z) {
            source.position.z
        } else {
            0.0
        }) + change.position.z,
    );
    let yaw = (if has(Relative::YRot) {
        source.y_rot
    } else {
        0.0
    }) + change.y_rot;
    let pitch = ((if has(Relative::XRot) {
        source.x_rot
    } else {
        0.0
    }) + change.x_rot)
        .clamp(-90.0, 90.0);
    let mut motion = util_vec(source.delta_movement);
    if has(Relative::RotateDelta) {
        motion = motion.x_rot(f64::from(source.x_rot - pitch).to_radians() as f32);
        motion = motion.y_rot(f64::from(source.y_rot - yaw).to_radians() as f32);
    }
    let motion = Vec3::new(
        if has(Relative::DeltaX) {
            motion.x + change.delta_movement.x
        } else {
            change.delta_movement.x
        },
        if has(Relative::DeltaY) {
            motion.y + change.delta_movement.y
        } else {
            change.delta_movement.y
        },
        if has(Relative::DeltaZ) {
            motion.z + change.delta_movement.z
        } else {
            change.delta_movement.z
        },
    );
    PositionMoveRotation {
        position: wire_vec(position),
        delta_movement: wire_vec(motion),
        y_rot: yaw,
        x_rot: pitch,
    }
}
pub fn position_move_rotation(
    position: Vec3,
    motion: Vec3,
    yaw: f32,
    pitch: f32,
) -> PositionMoveRotation {
    PositionMoveRotation {
        position: wire_vec(position),
        delta_movement: wire_vec(motion),
        y_rot: yaw,
        x_rot: pitch,
    }
}
fn wire_vec(value: Vec3) -> ferraloom_protocol::play::Vec3 {
    ferraloom_protocol::play::Vec3 {
        x: value.x,
        y: value.y,
        z: value.z,
    }
}
fn util_vec(value: ferraloom_protocol::play::Vec3) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

/// Port of ServerGamePacketListenerImpl's gameplay-side teleport bookkeeping.
/// Connections own sockets; this is the sole counter and pending destination.
#[derive(Debug, Default)]
pub struct TeleportState {
    pub awaiting_teleport: i32,
    pub awaiting_teleport_time: i32,
    pub awaiting_position: Option<Vec3>,
}
impl TeleportState {
    fn begin(&mut self, tick: u64) {
        self.awaiting_teleport_time = tick as i32;
        self.awaiting_teleport = self.awaiting_teleport.wrapping_add(1);
        if self.awaiting_teleport == i32::MAX {
            self.awaiting_teleport = 0;
        }
    }
    pub fn resend_due(&self, tick: u64) -> bool {
        self.awaiting_position.is_some()
            && (tick as i32).wrapping_sub(self.awaiting_teleport_time) > 20
    }
}

/// Port of Entity#getKnownMovement and ServerPlayer#getKnownMovement.
pub fn known_movement(entity: EntityRef, level: &Level) -> Result<Vec3, LevelError> {
    level.entity_behavior(entity)?.known_movement(entity, level)
}
pub(crate) fn player_known_movement(entity: EntityRef, level: &Level) -> Result<Vec3, LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    if let Some(vehicle) = player.vehicle.get()
        && level
            .entity_behavior(vehicle)?
            .controlling_passenger(vehicle, level)?
            != Some(entity)
    {
        return known_movement(vehicle, level);
    }
    Ok(player.known_movement)
}
pub(crate) fn base_known_movement<E: super::Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<Vec3, LevelError> {
    if let Some(controller) = behavior.controlling_passenger(entity, level)?
        && level.entity::<ServerPlayer>(controller).is_ok()
        && behavior.is_alive(entity, level)?
    {
        return known_movement(controller, level);
    }
    Ok(level.entity::<EntityData>(entity)?.velocity)
}

fn finish_player_teleport(
    player: &mut ServerPlayer,
    absolute: PositionMoveRotation,
    destination: PositionMoveRotation,
    relatives: i32,
) -> cb::Packet {
    player.set_y_rot(absolute.y_rot);
    player.y_head_rot = absolute.y_rot;
    player.set_x_rot(absolute.x_rot);
    player.set_old_pos_and_rot();
    player.velocity = util_vec(absolute.delta_movement);
    player.movement_this_tick.clear();
    player.teleport.awaiting_position = Some(player.position);
    cb::Packet::PlayerPosition(cb::PlayerPosition {
        id: VarInt(player.teleport.awaiting_teleport),
        change: destination,
        relatives: relatives & Relative::ALL,
    })
}
impl ServerPlayer {
    /// Initial placement uses the same counter, state transition and packet before
    /// the player has been registered in the region's spatial indexes.
    pub fn teleport_before_registration(
        &mut self,
        destination: PositionMoveRotation,
        relatives: i32,
    ) -> cb::Packet {
        self.teleport.begin(self.connection_ticks);
        let source =
            position_move_rotation(self.position, self.known_movement, self.yaw, self.pitch);
        let absolute = calculate_absolute(source, destination.clone(), relatives);
        self.set_pos(util_vec(absolute.position));
        finish_player_teleport(self, absolute, destination, relatives)
    }
}

/// Port of ServerGamePacketListenerImpl#teleport. Mutates canonical state and
/// enqueues the real packet synchronously; no later effect constructs teleports.
pub fn teleport_player(
    entity: EntityRef,
    destination: PositionMoveRotation,
    relatives: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    let motion = known_movement(entity, level)?;
    let source = {
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.teleport.begin(player.connection_ticks);
        position_move_rotation(player.position, motion, player.yaw, player.pitch)
    };
    let absolute = calculate_absolute(source, destination.clone(), relatives);
    level.set_entity_pos(entity, util_vec(absolute.position))?;
    let packet = finish_player_teleport(
        level.entity_mut::<ServerPlayer>(entity)?,
        absolute,
        destination,
        relatives,
    );
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![entity],
        packet: Box::new(packet),
    });
    Ok(())
}

/// Port of Entity#absSnapTo, including coordinate clamps and old rotation.
pub fn abs_snap_to(
    entity: EntityRef,
    position: Vec3,
    yaw: f32,
    pitch: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    let position = Vec3::new(
        position.x.clamp(-3.0e7, 3.0e7),
        position.y,
        position.z.clamp(-3.0e7, 3.0e7),
    );
    level.entity_mut::<EntityData>(entity)?.old_position = position;
    level.set_entity_pos(entity, position)?;
    level
        .entity_mut::<EntityData>(entity)?
        .abs_snap_rotation_to(yaw, pitch);
    Ok(())
}

/// Port of Entity#snapTo. Passenger teleportation preserves velocity and head yaw.
pub fn snap_to(entity: EntityRef, position: Vec3, level: &mut Level) -> Result<(), LevelError> {
    level.set_entity_pos(entity, position)?;
    level
        .entity_mut::<EntityData>(entity)?
        .set_old_pos_and_rot();
    Ok(())
}

/// Port of Entity#teleportRelative and ServerPlayer's listener override.
pub fn teleport_relative(
    entity: EntityRef,
    delta: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_behavior(entity)?
        .teleport_relative(entity, delta, level)
}
pub(crate) fn base_teleport_relative(
    entity: EntityRef,
    delta: Vec3,
    level: &mut Level,
) -> Result<(), LevelError> {
    let position = level.entity::<EntityData>(entity)?.position + delta;
    snap_to(entity, position, level)?;
    teleport_passengers(entity, level)
}
fn teleport_passengers(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let passengers = level.entity::<EntityData>(entity)?.passengers.clone();
    for &passenger in &passengers {
        if let Some(position) = super::lifecycle::rider_position(entity, passenger, level)? {
            snap_to(passenger, position, level)?;
        }
    }
    for passenger in passengers {
        teleport_passengers(passenger, level)?;
    }
    Ok(())
}

/// Port of the listener's accepted teleport snap, before its movement validation.
pub fn accept_teleport(entity: EntityRef, id: i32, level: &mut Level) -> Result<bool, LevelError> {
    let player = level.entity::<ServerPlayer>(entity)?;
    if player.teleport.awaiting_teleport != id {
        return Ok(false);
    }
    let Some(position) = player.teleport.awaiting_position else {
        return Ok(false);
    };
    let bounded = Vec3::new(
        position.x.clamp(-3.0e7, 3.0e7),
        position.y,
        position.z.clamp(-3.0e7, 3.0e7),
    );
    level.entity_mut::<EntityData>(entity)?.old_position = bounded;
    level.set_entity_pos(entity, bounded)?;
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    let (yaw, pitch) = (player.yaw, player.pitch);
    player.abs_snap_rotation_to(yaw, pitch);
    player.last_good = position;
    player.teleport.awaiting_position = None;
    Ok(true)
}
