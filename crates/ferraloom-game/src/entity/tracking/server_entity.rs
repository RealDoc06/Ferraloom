//! Port of ServerEntity for every implemented entity family.
use super::codec::{UpdateInterval, VecDeltaCodec, encoding_precision_loss};
use crate::{
    entity::{EntityData, EntityRef, EntityState},
    level::{Level, LevelError},
};
use ferraloom_protocol::{
    codec::{Angle, VarInt},
    entity_data::{DataValue, EntityDataItems},
    play::{
        self, PositionPath, VecDelta,
        clientbound::{self as packet, Packet},
    },
};
use ferraloom_util::{Vec3, mth};

#[derive(Debug)]
pub enum TrackingError {
    Level(LevelError),
}
impl From<LevelError> for TrackingError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl TrackingError {
    pub(crate) fn into_level(self, _level: &Level) -> LevelError {
        match self {
            Self::Level(error) => error,
        }
    }
}
impl std::fmt::Display for TrackingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TrackingError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingAudience {
    Players,
    PlayersAndSelf,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TrackingPacket {
    pub audience: TrackingAudience,
    pub packet: Packet,
}
impl TrackingPacket {
    fn players(packet: Packet) -> Self {
        Self {
            audience: TrackingAudience::Players,
            packet,
        }
    }
    fn with_self(packet: Packet) -> Self {
        Self {
            audience: TrackingAudience::PlayersAndSelf,
            packet,
        }
    }
}

/// Tracking keeps handles and prior wire state. Simulation data remains solely
/// in Level's canonical EntityState; encoding runs after scoped borrows end.
pub struct ServerEntity {
    pub entity: EntityRef,
    update_interval: UpdateInterval,
    track_delta: bool,
    position_codec: VecDeltaCodec,
    last_sent_y_rot: i8,
    last_sent_x_rot: i8,
    last_sent_y_head_rot: i8,
    last_sent_movement: Vec3,
    tick_count: i32,
    teleport_delay: i32,
    last_passengers: Vec<EntityRef>,
    was_riding: bool,
    was_on_ground: bool,
    tracked_data_values: Option<Vec<DataValue>>,
}
impl ServerEntity {
    pub fn new(
        entity: EntityRef,
        level: &Level,
        update_interval: UpdateInterval,
        track_delta: bool,
    ) -> Result<Self, TrackingError> {
        let state = level.entity_state(entity)?;
        let base = state.as_entity();
        Ok(Self {
            entity,
            update_interval,
            track_delta,
            position_codec: VecDeltaCodec {
                base: base.position,
            },
            last_sent_y_rot: mth::pack_degrees(base.yaw),
            last_sent_x_rot: mth::pack_degrees(base.pitch),
            last_sent_y_head_rot: mth::pack_degrees(
                level.entity_behavior(entity)?.y_head_rot(entity, level)?,
            ),
            last_sent_movement: base.velocity,
            tick_count: 0,
            teleport_delay: 0,
            last_passengers: Vec::new(),
            was_riding: false,
            was_on_ground: base.on_ground,
            tracked_data_values: base.entity_data.non_default_values(),
        })
    }
    pub fn position_base(&self) -> Vec3 {
        self.position_codec.base
    }
    pub fn last_sent_movement(&self) -> Vec3 {
        self.last_sent_movement
    }
    /// Native pairing order is spawn, cached non-default metadata, attributes,
    /// equipment, passengers and the mounted vehicle's passengers.
    #[cfg(test)]
    pub fn pairing_data(&self, level: &mut Level) -> Result<Vec<Packet>, TrackingError> {
        level
            .entity_behavior(self.entity)?
            .update_data_before_sync(self.entity, level)?;
        self.pairing_data_after_sync(level)
    }
    /// Read pairing state after updateDataBeforeSync has returned. The live
    /// tracker keeps its canonical entry in the Region during that callback.
    pub(crate) fn pairing_data_after_sync(
        &self,
        level: &Level,
    ) -> Result<Vec<Packet>, TrackingError> {
        let state = level.entity_state(self.entity)?;
        let base = state.as_entity();
        let position = self.position_codec.base;
        let data = match state {
            EntityState::FallingBlock(block) => i32::from(block.block_state.id()),
            _ => 0,
        };
        let mut packets = vec![Packet::AddEntity(packet::AddEntity {
            id: VarInt(base.network_id),
            uuid: base.uuid,
            entity_type: play::EntityTypeId(i32::from(state.entity_type().protocol_id())),
            x: position.x,
            y: position.y,
            z: position.z,
            movement: play::LpVec3(play::Vec3 {
                x: self.last_sent_movement.x,
                y: self.last_sent_movement.y,
                z: self.last_sent_movement.z,
            }),
            x_rot: Angle(self.last_sent_x_rot as u8),
            y_rot: Angle(self.last_sent_y_rot as u8),
            y_head_rot: Angle(self.last_sent_y_head_rot as u8),
            data: VarInt(data),
        })];
        if let Some(data) = &self.tracked_data_values {
            packets.push(Packet::SetEntityData(packet::SetEntityData {
                id: VarInt(base.network_id),
                packed_items: EntityDataItems(data.clone()),
            }));
        }
        if let Some(living) = state.as_living() {
            let values: Vec<_> = living
                .attributes
                .syncable_attributes()
                .map(|attribute| attribute.snapshot())
                .collect();
            if !values.is_empty() {
                packets.push(Packet::UpdateAttributes(packet::UpdateAttributes {
                    entity_id: VarInt(base.network_id),
                    values,
                }));
            }
            let mut slots = Vec::new();
            for slot in crate::entity::player::equipment::SLOTS {
                let item = crate::entity::living::item_by_slot(self.entity, slot, level)?;
                if !item.is_empty() {
                    slots.push(packet::EquipmentEntry { slot, item });
                }
            }
            if !slots.is_empty() {
                packets.push(Packet::SetEquipment(packet::SetEquipment {
                    entity: VarInt(base.network_id),
                    slots,
                }));
            }
        }
        if !base.passengers.is_empty() {
            packets.push(passengers_packet(self.entity, level)?);
        }
        if let Some(vehicle) = base.vehicle.get() {
            packets.push(passengers_packet(vehicle, level)?);
        }
        // Mob implements Leashable. Delayed UUID/fence attachments are not
        // isLeashed until their canonical holder has been resolved.
        if let Some(holder) = state
            .as_mob()
            .and_then(|mob| mob.leash_data.as_ref())
            .and_then(|leash| leash.holder)
        {
            packets.push(Packet::SetEntityLink(packet::SetEntityLink {
                source_id: base.network_id,
                dest_id: level.entity::<EntityData>(holder)?.network_id,
            }));
        }
        // Native sendPairingData sends no active-effect packets. Those belong
        // to PlayerList#sendActiveEffects and ServerPlayer#startRiding.
        Ok(packets)
    }
    #[cfg(test)]
    pub fn removal_packet(&self, level: &Level) -> Result<Packet, TrackingError> {
        Ok(Packet::RemoveEntities(packet::RemoveEntities {
            entity_ids: vec![VarInt(level.entity::<EntityData>(self.entity)?.network_id)],
        }))
    }
    fn send_dirty_entity_data(&mut self, state: &mut EntityState, out: &mut Vec<TrackingPacket>) {
        let base = state.as_entity_mut();
        if let Some(data) = base.entity_data.pack_dirty() {
            self.tracked_data_values = base.entity_data.non_default_values();
            out.push(TrackingPacket::with_self(Packet::SetEntityData(
                packet::SetEntityData {
                    id: VarInt(base.network_id),
                    packed_items: EntityDataItems(data),
                },
            )));
        }
        let network_id = state.as_entity().network_id;
        if let Some(living) = state.as_living_mut() {
            let values: Vec<_> = living
                .attributes
                .attributes_to_sync()
                .map(|attribute| attribute.snapshot())
                .collect();
            if !values.is_empty() {
                out.push(TrackingPacket::with_self(Packet::UpdateAttributes(
                    packet::UpdateAttributes {
                        entity_id: VarInt(network_id),
                        values,
                    },
                )));
            }
            living.attributes.clear_attributes_to_sync();
        }
    }
    #[cfg(test)]
    pub fn send_changes(
        &mut self,
        level: &mut Level,
    ) -> Result<Vec<TrackingPacket>, TrackingError> {
        level
            .entity_behavior(self.entity)?
            .update_data_before_sync(self.entity, level)?;
        let passengers = self.passenger_update(level)?;
        let (mut out, update_head) =
            self.send_changes_before_head(level.entity_state_mut(self.entity)?, passengers);
        if update_head {
            let head = level
                .entity_behavior(self.entity)?
                .y_head_rot(self.entity, level)?;
            self.send_head_rotation(level.entity_state_mut(self.entity)?, head, &mut out);
        }
        self.finish_changes(level.entity_state_mut(self.entity)?, &mut out);
        Ok(out)
    }
    pub(crate) fn passenger_update(&self, level: &Level) -> Result<Option<Packet>, TrackingError> {
        let changed = level.entity::<EntityData>(self.entity)?.passengers != self.last_passengers;
        changed
            .then(|| passengers_packet(self.entity, level).map_err(Into::into))
            .transpose()
    }
    /// Callback-free mutations of canonical entity and wire state, with short
    /// disjoint Region borrows. No tracker entry leaves the canonical map.
    pub(crate) fn send_changes_before_head(
        &mut self,
        state: &mut EntityState,
        passengers: Option<Packet>,
    ) -> (Vec<TrackingPacket>, bool) {
        let mut out = Vec::new();
        let base = state.as_entity();
        if let Some(packet) = passengers {
            out.push(TrackingPacket::players(packet));
            self.last_passengers = base.passengers.clone();
        }
        let position = base.position;
        if let Some(living) = state.as_living_mut() {
            living
                .interpolation_tracker
                .update_tracking(position, &mut living.parent.sync_position);
        }
        let base = state.as_entity();
        if base.sync_position {
            self.tick_count = self.update_interval.next_interval(self.tick_count);
            state.as_entity_mut().sync_position = false;
        }
        let base = state.as_entity();
        let update = base.needs_sync
            || self.update_interval.test(self.tick_count)
            || base.entity_data.is_dirty();
        if update {
            let (y_rot, x_rot) = (mth::pack_degrees(base.yaw), mth::pack_degrees(base.pitch));
            let rotation_changed = (i32::from(y_rot) - i32::from(self.last_sent_y_rot)).abs() >= 1
                || (i32::from(x_rot) - i32::from(self.last_sent_x_rot)).abs() >= 1;
            let position = base.position;
            if base.vehicle.get().is_some() {
                if rotation_changed {
                    out.push(TrackingPacket::players(Packet::MoveEntityRot(
                        packet::MoveEntityRot {
                            entity_id: VarInt(base.network_id),
                            on_ground: base.on_ground,
                            y_rot: Angle(y_rot as u8),
                            x_rot: Angle(x_rot as u8),
                        },
                    )));
                    self.last_sent_y_rot = y_rot;
                    self.last_sent_x_rot = x_rot;
                }
                self.position_codec.base = position;
                if let Some(living) = state.as_living_mut() {
                    living.interpolation_tracker.clear();
                }
                self.send_dirty_entity_data(state, &mut out);
                self.was_riding = true;
            } else {
                self.teleport_delay = self.teleport_delay.wrapping_add(1);
                let path = if let Some(living) = state.as_living_mut() {
                    let path = living.interpolation_tracker.position_path(position);
                    living.interpolation_tracker.clear();
                    path
                } else {
                    PositionPath::Linear(play::Vec3 {
                        x: position.x,
                        y: position.y,
                        z: position.z,
                    })
                };
                let send_position = self.position_codec.delta(position).length_sqr()
                    >= 7.62939453125E-6
                    || self.tick_count % 60 == 0;
                let (movement_packet, has_position, has_rotation) = self.create_move_packet(
                    state,
                    &path,
                    send_position,
                    rotation_changed,
                    y_rot,
                    x_rot,
                );
                let base = state.as_entity();
                let fall_flying = state.as_living().is_some() && base.shared_flag(7);
                if base.needs_sync || self.track_delta || fall_flying {
                    let movement = base.velocity;
                    let diff = movement.distance_to_sqr(self.last_sent_movement);
                    if diff > 1.0E-7 || diff > 0.0 && movement.length_sqr() == 0.0 {
                        self.last_sent_movement = movement;
                        out.push(TrackingPacket::players(Packet::SetEntityMotion(
                            packet::SetEntityMotion {
                                id: VarInt(base.network_id),
                                movement: play::LpVec3(play::Vec3 {
                                    x: movement.x,
                                    y: movement.y,
                                    z: movement.z,
                                }),
                            },
                        )));
                    }
                }
                if let Some(packet) = movement_packet {
                    out.push(TrackingPacket::players(packet));
                    self.send_dirty_entity_data(state, &mut out);
                    if has_position {
                        self.position_codec.base = position;
                    }
                    if has_rotation {
                        self.last_sent_y_rot = y_rot;
                        self.last_sent_x_rot = x_rot;
                    }
                } else {
                    self.send_dirty_entity_data(state, &mut out);
                }
                self.was_riding = false;
            }
        }
        (out, update)
    }
    pub(crate) fn send_head_rotation(
        &mut self,
        state: &mut EntityState,
        head: f32,
        out: &mut Vec<TrackingPacket>,
    ) {
        let y_head_rot = mth::pack_degrees(head);
        if (i32::from(y_head_rot) - i32::from(self.last_sent_y_head_rot)).abs() >= 1 {
            out.push(TrackingPacket::players(Packet::RotateHead(
                packet::RotateHead {
                    entity_id: VarInt(state.as_entity().network_id),
                    y_head_rot: Angle(y_head_rot as u8),
                },
            )));
            self.last_sent_y_head_rot = y_head_rot;
        }
        state.as_entity_mut().needs_sync = false;
    }
    pub(crate) fn finish_changes(
        &mut self,
        state: &mut EntityState,
        out: &mut Vec<TrackingPacket>,
    ) {
        self.tick_count = self.tick_count.wrapping_add(1);
        let base = state.as_entity_mut();
        if base.sync_velocity {
            base.sync_velocity = false;
            out.push(TrackingPacket::with_self(Packet::SetEntityMotion(
                packet::SetEntityMotion {
                    id: VarInt(base.network_id),
                    movement: play::LpVec3(play::Vec3 {
                        x: base.velocity.x,
                        y: base.velocity.y,
                        z: base.velocity.z,
                    }),
                },
            )));
        }
    }
    fn create_move_packet(
        &mut self,
        state: &EntityState,
        position: &PositionPath,
        send_position: bool,
        send_rotation: bool,
        y_rot: i8,
        x_rot: i8,
    ) -> (Option<Packet>, bool, bool) {
        let base = state.as_entity();
        let absolute = |base: &EntityData| {
            Packet::EntityPositionSync(packet::EntityPositionSync {
                id: VarInt(base.network_id),
                position: position.clone(),
                y_rot: base.yaw,
                x_rot: base.pitch,
                on_ground: base.on_ground,
            })
        };
        if base.requires_precise_position
            || self.teleport_delay > 400
            || self.was_riding
            || self.was_on_ground != base.on_ground
        {
            self.was_on_ground = base.on_ground;
            self.teleport_delay = 0;
            return (Some(absolute(base)), true, true);
        }
        if send_position {
            let Some(delta) = self.position_codec.try_encode_path(position) else {
                return (Some(absolute(base)), true, true);
            };
            if matches!(state, EntityState::Item(_)) && item_requires_full_precision(base, &delta) {
                return (Some(absolute(base)), true, true);
            }
            let delta = play::EntityDelta {
                on_ground: base.on_ground,
                delta,
            };
            let packet = if send_rotation {
                Packet::MoveEntityPosRot(packet::MoveEntityPosRot {
                    entity_id: VarInt(base.network_id),
                    delta,
                    y_rot: Angle(y_rot as u8),
                    x_rot: Angle(x_rot as u8),
                })
            } else {
                Packet::MoveEntityPos(packet::MoveEntityPos {
                    entity_id: VarInt(base.network_id),
                    delta,
                })
            };
            return (Some(packet), true, send_rotation);
        }
        (
            send_rotation.then_some(Packet::MoveEntityRot(packet::MoveEntityRot {
                entity_id: VarInt(base.network_id),
                on_ground: base.on_ground,
                y_rot: Angle(y_rot as u8),
                x_rot: Angle(x_rot as u8),
            })),
            false,
            send_rotation,
        )
    }
}
fn passengers_packet(entity: EntityRef, level: &Level) -> Result<Packet, LevelError> {
    let base = level.entity::<EntityData>(entity)?;
    let passengers = base
        .passengers
        .iter()
        .map(|&id| {
            level
                .entity::<EntityData>(id)
                .map(|base| VarInt(base.network_id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Packet::SetPassengers(packet::SetPassengers {
        vehicle: VarInt(base.network_id),
        passengers,
    }))
}
fn item_requires_full_precision(base: &EntityData, delta: &VecDelta) -> bool {
    let (has_x, has_z) = match delta {
        VecDelta::Linear { xa, za, .. } => (*xa != 0, *za != 0),
        VecDelta::Stepped(steps) => (
            steps.iter().any(|step| step.xa != 0),
            steps.iter().any(|step| step.za != 0),
        ),
    };
    let x_loss = encoding_precision_loss(base.position.x) != 0.0;
    let z_loss = encoding_precision_loss(base.position.z) != 0.0;
    base.vertical_collision && (has_x && x_loss || has_z && z_loss)
        || base.horizontal_collision && (has_x && z_loss || has_z && x_loss)
}

#[cfg(test)]
mod tests;
