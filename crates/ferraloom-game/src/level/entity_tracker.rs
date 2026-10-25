//! Region-owned ChunkMap entity tracking. Entries remain reachable during world
//! callbacks; each step reborrows the entry by its native network id.
use super::{Level, LevelError, effects::LevelEffect};
use crate::entity::{
    EntityData, EntityRef, EntityState,
    player::ServerPlayer,
    tracking::{
        PlayerTrackingView, TrackedEntity, TrackingAudience, TrackingError, TrackingPacket,
    },
};
use ferraloom_protocol::{
    codec::VarInt,
    play::{
        ClientboundMessage,
        clientbound::{self as packet, Packet},
    },
};
use ferraloom_util::{SectionPos, collections::Int2ObjectOpenHashMap};

/// Port of ChunkMap#entityMap and the immutable server tracking settings.
pub struct EntityTracker {
    entries: Int2ObjectOpenHashMap<TrackedEntity>,
    /// Port of ChunkMap#playerMap's ignored flags; this map is lookup-only.
    player_map: std::collections::HashMap<EntityRef, bool>,
    server_view_distance: i32,
    broadcast_range_percentage: i32,
}
impl Default for EntityTracker {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            player_map: Default::default(),
            server_view_distance: 10,
            broadcast_range_percentage: 100,
        }
    }
}
impl Level {
    /// Publish the canonical server properties before entities enter the level.
    pub fn configure_entity_tracking(&mut self, view_distance: i32, range_percentage: i32) {
        let tracker = &mut self.region_mut().entity_tracker;
        tracker.server_view_distance = view_distance.clamp(2, 32);
        tracker.broadcast_range_percentage = range_percentage;
    }
    /// Port of PlayerMap#ignoredOrUnknown. Ignored players keep their client view
    /// and entity audiences, but do not contribute DistanceManager player tickets.
    pub fn player_ignored_by_chunk_map(&self, player: EntityRef) -> Result<bool, LevelError> {
        self.entity::<ServerPlayer>(player)?;
        Ok(self
            .region()
            .entity_tracker
            .player_map
            .get(&player)
            .copied()
            .unwrap_or(true))
    }
    /// Port of ChunkMap#updatePlayerStatus and move's section/ignored transition.
    /// Reevaluate skipPlayer even when a movement stays in the same section.
    pub fn update_player_tracking_section(
        &mut self,
        player: EntityRef,
        section: SectionPos,
    ) -> Result<(), LevelError> {
        let state = self.entity::<ServerPlayer>(player)?;
        let ignored = state.game_mode == crate::entity::player::GameType::Spectator
            && !self.rules().spectators_generate_chunks;
        let previous = self
            .region()
            .entity_tracker
            .player_map
            .get(&player)
            .copied();
        if state.chunk_tracking_section == Some(section) && previous == Some(ignored) {
            return Ok(());
        }
        if previous.is_none() {
            self.region_mut()
                .entity_tracker
                .player_map
                .insert(player, ignored);
        }
        self.entity_mut::<ServerPlayer>(player)?
            .chunk_tracking_section = Some(section);
        if (!ignored || previous == Some(false))
            && let Some(driver) = &self.acquire_driver
        {
            driver.update_player_tracking_status(player, (!ignored).then_some(section))?;
        }
        self.region_mut()
            .entity_tracker
            .player_map
            .insert(player, ignored);
        Ok(())
    }
    /// Port of ChunkMap#updateChunkTracking/applyChunkTrackingView. Packet
    /// audiences and pending chunks are captured before later world callbacks.
    pub fn update_player_chunk_tracking(&mut self, player: EntityRef) -> Result<(), LevelError> {
        let state = self.entity::<ServerPlayer>(player)?;
        let center = state.chunk_position();
        let distance = state
            .view_distance
            .clamp(2, self.region().entity_tracker.server_view_distance);
        let desired: std::collections::HashSet<_> =
            super::distance_manager::tracking_view(center, distance)
                .into_iter()
                .collect();
        let new_center = state.chunk_tracking_center != Some(center);
        let mut leaving: Vec<_> = state
            .chunk_tracking_view
            .difference(&desired)
            .copied()
            .collect();
        leaving.sort_by_key(|pos| pos.pack());
        if new_center {
            self.emit_tracking_message(
                vec![player],
                ClientboundMessage::Packet(Box::new(Packet::SetChunkCacheCenter(
                    packet::SetChunkCacheCenter {
                        x: VarInt(center.x),
                        z: VarInt(center.z),
                    },
                ))),
            );
        }
        for pos in leaving {
            let forget = self
                .entity_mut::<ServerPlayer>(player)?
                .chunk_sender
                .drop_chunk(pos);
            if forget && self.entity_behavior(player)?.is_alive(player, self)? {
                self.emit_tracking_message(
                    vec![player],
                    ClientboundMessage::Packet(Box::new(Packet::ForgetLevelChunk(
                        packet::ForgetLevelChunk {
                            pos: ferraloom_protocol::play::ChunkPos { x: pos.x, z: pos.z },
                        },
                    ))),
                );
            }
        }
        let state = self.entity_mut::<ServerPlayer>(player)?;
        for &pos in desired.difference(&state.chunk_tracking_view) {
            state.chunk_sender.pending.insert(pos);
        }
        state.chunk_tracking_view = desired;
        state.chunk_tracking_center = Some(center);
        state.chunk_tracking_distance = Some(distance);
        Ok(())
    }
    fn remove_player_tracking_status(&mut self, player: EntityRef) -> Result<(), LevelError> {
        let was_ignored = self
            .region_mut()
            .entity_tracker
            .player_map
            .remove(&player)
            .unwrap_or(true);
        if !was_ignored && let Some(driver) = &self.acquire_driver {
            driver.update_player_tracking_status(player, None)?;
        }
        self.entity_mut::<ServerPlayer>(player)?
            .chunk_tracking_section = None;
        // Native applyChunkTrackingView(EMPTY) drops chunks before entity removal.
        let mut positions: Vec<_> = self
            .entity::<ServerPlayer>(player)?
            .chunk_tracking_view
            .iter()
            .copied()
            .collect();
        positions.sort_by_key(|pos| pos.pack());
        for pos in positions {
            if self
                .entity_mut::<ServerPlayer>(player)?
                .chunk_sender
                .drop_chunk(pos)
                && self.entity_behavior(player)?.is_alive(player, self)?
            {
                self.emit_tracking_message(
                    vec![player],
                    ClientboundMessage::Packet(Box::new(Packet::ForgetLevelChunk(
                        packet::ForgetLevelChunk {
                            pos: ferraloom_protocol::play::ChunkPos { x: pos.x, z: pos.z },
                        },
                    ))),
                );
            }
        }
        let state = self.entity_mut::<ServerPlayer>(player)?;
        state.chunk_tracking_view.clear();
        state.chunk_tracking_center = None;
        state.chunk_tracking_distance = None;
        Ok(())
    }
    fn tracked_entity(&self, network: i32) -> Result<&TrackedEntity, TrackingError> {
        self.region()
            .entity_tracker
            .entries
            .get(&network)
            .ok_or(LevelError::MissingTrackedEntity(network).into())
    }
    fn tracker_ids(&self) -> Vec<i32> {
        self.region()
            .entity_tracker
            .entries
            .keys()
            .copied()
            .collect()
    }
    fn tracking_players(&self) -> Vec<EntityRef> {
        self.region()
            .entities
            .lookup
            .all()
            .into_iter()
            .filter(|&entity| matches!(self.entity_state(entity), Ok(EntityState::Player(_))))
            .collect()
    }
    fn tracking_view(
        &self,
        network: i32,
        player: EntityRef,
    ) -> Result<PlayerTrackingView, TrackingError> {
        let tracker = &self.region().entity_tracker;
        let entity = self.tracked_entity(network)?.server_entity.entity;
        let chunk = self.entity::<EntityData>(entity)?.chunk_position;
        let state = self.entity::<ServerPlayer>(player)?;
        Ok(PlayerTrackingView {
            player,
            server_view_distance: tracker.server_view_distance,
            broadcast_range_percentage: tracker.broadcast_range_percentage,
            chunk_tracked: state.chunk_tracking_view.contains(&chunk)
                && !state.chunk_sender.pending.contains(&chunk),
        })
    }
    fn emit_tracking_message(&mut self, recipients: Vec<EntityRef>, message: ClientboundMessage) {
        if recipients.is_empty() {
            return;
        }
        for packet in message.into_packets() {
            self.emit(LevelEffect::PlayerPacket {
                recipients: recipients.clone(),
                packet: Box::new(packet),
            });
        }
    }
    /// Port of ChunkMap.TrackedEntity#updatePlayer and ServerEntity#addPairing.
    /// The bundle is emitted before startSeenByPlayer, so nested broadcasts see
    /// both the installed audience and all preceding pairing packets.
    pub fn update_tracked_player(
        &mut self,
        network: i32,
        player: EntityRef,
    ) -> Result<(), TrackingError> {
        let view = self.tracking_view(network, player)?;
        let tracked = self.tracked_entity(network)?;
        let entity = tracked.server_entity.entity;
        if entity == player {
            return Ok(());
        }
        let visible = tracked.visible_to(self, view)?;
        if visible {
            if self.tracked_entity(network)?.seen_by().contains(&player) {
                return Ok(());
            }
            self.region_mut()
                .entity_tracker
                .entries
                .get_mut(&network)
                .unwrap()
                .seen_by
                .push(player);
            self.entity_behavior(entity)?
                .update_data_before_sync(entity, self)?;
            let packets = self
                .tracked_entity(network)?
                .server_entity
                .pairing_data_after_sync(self)?;
            self.emit_tracking_message(vec![player], ClientboundMessage::Bundle(packets));
            self.entity_behavior(entity)?
                .start_seen_by_player(entity, player, self)?;
        } else {
            self.remove_tracked_player(network, player)?;
        }
        Ok(())
    }
    fn remove_tracked_player(
        &mut self,
        network: i32,
        player: EntityRef,
    ) -> Result<(), TrackingError> {
        let tracked = self
            .region_mut()
            .entity_tracker
            .entries
            .get_mut(&network)
            .ok_or(LevelError::MissingEntity(player))?;
        let Some(index) = tracked.seen_by.iter().position(|&seen| seen == player) else {
            return Ok(());
        };
        tracked.seen_by.remove(index);
        let entity = tracked.server_entity.entity;
        self.entity_behavior(entity)?
            .stop_seen_by_player(entity, player, self)?;
        self.emit_tracking_message(
            vec![player],
            ClientboundMessage::Packet(Box::new(Packet::RemoveEntities(packet::RemoveEntities {
                entity_ids: vec![VarInt(network)],
            }))),
        );
        Ok(())
    }
    /// Port of ChunkMap#addEntity. The entity is published before pairing hooks.
    pub fn add_tracked_entity(&mut self, entity: EntityRef) -> Result<(), TrackingError> {
        let state = self.entity_state(entity)?;
        if state.entity_type().data().tracking_range == 0 {
            return Ok(());
        }
        let network = state.as_entity().network_id;
        if self.region().entity_tracker.entries.contains_key(&network) {
            return Err(LevelError::DuplicateEntity(entity).into());
        }
        let tracked = TrackedEntity::new(entity, self)?;
        self.region_mut()
            .entity_tracker
            .entries
            .insert(network, tracked);
        for player in self.tracking_players() {
            self.update_tracked_player(network, player)?;
        }
        if matches!(self.entity_state(entity)?, EntityState::Player(_)) {
            let section = SectionPos::from_position(self.entity::<EntityData>(entity)?.position);
            self.update_player_tracking_section(entity, section)?;
            self.entity_mut::<ServerPlayer>(entity)?
                .chunk_tracking_view
                .clear();
            self.entity_mut::<ServerPlayer>(entity)?
                .chunk_tracking_center = None;
            self.entity_mut::<ServerPlayer>(entity)?
                .chunk_tracking_distance = None;
            self.update_player_chunk_tracking(entity)?;
            for id in self.tracker_ids() {
                if id != network {
                    self.update_tracked_player(id, entity)?;
                }
            }
        }
        Ok(())
    }
    /// Port of ChunkMap#removeEntity, including the leaving player's audiences.
    pub fn remove_tracked_entity(&mut self, entity: EntityRef) -> Result<(), TrackingError> {
        let network = self.entity::<EntityData>(entity)?.network_id;
        if matches!(self.entity_state(entity)?, EntityState::Player(_)) {
            self.remove_player_tracking_status(entity)?;
            for id in self.tracker_ids() {
                self.remove_tracked_player(id, entity)?;
            }
        }
        // Native removes the map entry before broadcastRemoved. Keep only the
        // captured connection handles across its stopSeenByPlayer callbacks.
        if let Some(tracked) = self.region_mut().entity_tracker.entries.remove(&network) {
            for player in tracked.seen_by().to_vec() {
                self.entity_behavior(entity)?
                    .stop_seen_by_player(entity, player, self)?;
                self.emit_tracking_message(
                    vec![player],
                    ClientboundMessage::Packet(Box::new(Packet::RemoveEntities(
                        packet::RemoveEntities {
                            entity_ids: vec![VarInt(network)],
                        },
                    ))),
                );
            }
        }
        Ok(())
    }
    /// Port of ChunkMap#move's audience pass, before player ticket updates.
    pub fn move_tracked_player(&mut self, player: EntityRef) -> Result<(), TrackingError> {
        for id in self.tracker_ids() {
            if self.tracked_entity(id)?.server_entity.entity == player {
                for observer in self.tracking_players() {
                    self.update_tracked_player(id, observer)?;
                }
            } else {
                self.update_tracked_player(id, player)?;
            }
        }
        Ok(())
    }
    /// Port of ChunkMap#tick, after updateChunkTracking and before entity ticks.
    /// Frozen simulation still updates tracking; moved players get a final pass.
    pub fn tick_entity_tracker(&mut self) -> Result<(), TrackingError> {
        let mut moved_players = Vec::new();
        for id in self.tracker_ids() {
            let tracked = self.tracked_entity(id)?;
            let entity = tracked.server_entity.entity;
            let base = self.entity::<EntityData>(entity)?;
            let section = SectionPos::from_position(base.position);
            let changed = section != tracked.last_section_pos;
            if changed {
                for observer in self.tracking_players() {
                    self.update_tracked_player(id, observer)?;
                }
                if matches!(self.entity_state(entity)?, EntityState::Player(_)) {
                    moved_players.push(entity);
                }
                self.region_mut()
                    .entity_tracker
                    .entries
                    .get_mut(&id)
                    .unwrap()
                    .last_section_pos = section;
            }
            if changed
                || self.entity::<EntityData>(entity)?.needs_sync
                || self.is_entity_ticking_chunk(section.chunk())
            {
                self.send_tracked_entity_changes(id)?;
            }
        }
        for id in self.tracker_ids() {
            for &player in &moved_players {
                self.update_tracked_player(id, player)?;
            }
        }
        Ok(())
    }
    fn send_tracked_entity_changes(&mut self, network: i32) -> Result<(), TrackingError> {
        let entity = self.tracked_entity(network)?.server_entity.entity;
        self.entity_behavior(entity)?
            .update_data_before_sync(entity, self)?;
        let passengers = self
            .tracked_entity(network)?
            .server_entity
            .passenger_update(self)?;
        let (mut packets, update_head) = {
            let region = self.region_mut();
            let tracked = region.entity_tracker.entries.get_mut(&network).unwrap();
            let state = region
                .entities
                .get_mut(&entity)
                .ok_or(LevelError::MissingEntity(entity))?;
            tracked
                .server_entity
                .send_changes_before_head(state, passengers)
        };
        if update_head {
            let head = self.entity_behavior(entity)?.y_head_rot(entity, self)?;
            let region = self.region_mut();
            let tracked = region.entity_tracker.entries.get_mut(&network).unwrap();
            let state = region
                .entities
                .get_mut(&entity)
                .ok_or(LevelError::MissingEntity(entity))?;
            tracked
                .server_entity
                .send_head_rotation(state, head, &mut packets);
        }
        {
            let region = self.region_mut();
            let tracked = region.entity_tracker.entries.get_mut(&network).unwrap();
            let state = region
                .entities
                .get_mut(&entity)
                .ok_or(LevelError::MissingEntity(entity))?;
            tracked.server_entity.finish_changes(state, &mut packets);
        }
        for packet in packets {
            self.broadcast_tracked_packet(entity, packet)?;
        }
        Ok(())
    }
    /// Port of ChunkMap#sendToTrackingPlayers/sendToTrackingPlayersAndSelf.
    pub fn broadcast_tracked_packet(
        &mut self,
        entity: EntityRef,
        update: TrackingPacket,
    ) -> Result<(), TrackingError> {
        let network = self.entity::<EntityData>(entity)?.network_id;
        let Some(tracked) = self.region().entity_tracker.entries.get(&network) else {
            return Ok(());
        };
        let mut recipients = tracked.seen_by().to_vec();
        if update.audience == TrackingAudience::PlayersAndSelf
            && matches!(self.entity_state(entity)?, EntityState::Player(_))
        {
            recipients.push(entity);
        }
        self.emit_tracking_message(
            recipients,
            ClientboundMessage::Packet(Box::new(update.packet)),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{Entity, item::ItemEntity, player::GameType},
        level::Region,
    };
    use ferraloom_data::{EntityType, Item};
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{ChunkPos, Uuid, Vec3};

    struct ReentrantPairing;
    impl Entity for ReentrantPairing {
        fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
            crate::entity::trace_base_entity_refs(state.as_entity(), refs);
        }
        fn update_data_before_sync(
            &self,
            entity: EntityRef,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            // Reentry sees the actual entry and the newly installed audience.
            level
                .broadcast_tracked_packet(
                    entity,
                    TrackingPacket {
                        audience: TrackingAudience::Players,
                        packet: Packet::EntityEvent(packet::EntityEvent {
                            entity_id: level.entity::<EntityData>(entity)?.network_id,
                            event_id: 60,
                        }),
                    },
                )
                .unwrap();
            Ok(())
        }
        fn start_seen_by_player(
            &self,
            entity: EntityRef,
            _: EntityRef,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            level
                .broadcast_tracked_packet(
                    entity,
                    TrackingPacket {
                        audience: TrackingAudience::Players,
                        packet: Packet::EntityEvent(packet::EntityEvent {
                            entity_id: level.entity::<EntityData>(entity)?.network_id,
                            event_id: 61,
                        }),
                    },
                )
                .unwrap();
            Ok(())
        }
        fn stop_seen_by_player(
            &self,
            entity: EntityRef,
            player: EntityRef,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            let network = level.entity::<EntityData>(entity)?.network_id;
            level.emit_tracking_message(
                vec![player],
                ClientboundMessage::Packet(Box::new(Packet::EntityEvent(packet::EntityEvent {
                    entity_id: network,
                    event_id: 62,
                }))),
            );
            Ok(())
        }
    }
    fn fixture() -> (Level, EntityRef, EntityRef) {
        static BEHAVIORS: Behaviors =
            Behaviors::EMPTY.with_entity(EntityType::ITEM, &ReentrantPairing);
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            super::super::LevelContext::fixture(0, 63),
        );
        level.install_shared(Box::default());
        let viewer = level.next_entity_ref();
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "viewer".into(),
            12,
            Vec3::new(0.0, 64.0, 0.0),
            GameType::Spectator,
        );
        player.chunk_tracking_view.insert(ChunkPos::ZERO);
        level
            .add_entity(viewer, EntityState::Player(Box::new(player)))
            .unwrap();
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .remove(&ChunkPos::ZERO);
        level.take_effects();
        let entity = level.next_entity_ref();
        let mut item = ItemEntity::new(7, 3);
        item.parent.set_pos(Vec3::new(0.0, 64.0, 0.0));
        item.set_item(ItemStack::new(Item::STONE, 3, Default::default()));
        level
            .add_entity(entity, EntityState::Item(Box::new(item)))
            .unwrap();
        (level, viewer, entity)
    }
    #[test]
    fn pairing_entry_and_audience_remain_visible_through_both_callbacks() {
        let (mut level, viewer, entity) = fixture();
        level.add_tracked_entity(entity).unwrap();
        let packets: Vec<_> = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::PlayerPacket { recipients, packet } => {
                    assert_eq!(recipients, vec![viewer]);
                    Some(*packet)
                }
                _ => None,
            })
            .collect();
        assert!(matches!(&packets[0], Packet::EntityEvent(event) if event.event_id == 60));
        assert!(matches!(&packets[1], Packet::BundleDelimiter(_)));
        assert!(matches!(&packets[2], Packet::AddEntity(_)));
        assert!(matches!(&packets[3], Packet::SetEntityData(_)));
        assert!(matches!(&packets[4], Packet::BundleDelimiter(_)));
        assert!(matches!(&packets[5], Packet::EntityEvent(event) if event.event_id == 61));
        assert_eq!(packets.len(), 6);
    }
    #[test]
    fn pending_chunk_prevents_pairing_until_native_audience_refresh() {
        let (mut level, viewer, entity) = fixture();
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .insert(ChunkPos::ZERO);
        level.add_tracked_entity(entity).unwrap();
        assert!(
            level
                .take_effects()
                .iter()
                .all(|effect| !matches!(effect, LevelEffect::PlayerPacket { .. }))
        );
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .remove(&ChunkPos::ZERO);
        level.move_tracked_player(viewer).unwrap();
        assert!(level.take_effects().iter().any(|effect| matches!(effect, LevelEffect::PlayerPacket { packet, .. } if matches!(packet.as_ref(), Packet::AddEntity(_)))));
    }
    #[test]
    fn native_audience_boundary_pending_chunk_and_callback_packet_order() {
        let (mut level, viewer, entity) = fixture();
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .view_distance = 2;
        level
            .entity_mut::<EntityData>(viewer)
            .unwrap()
            .set_pos(Vec3::new(32.0, 100000.0, 0.0));
        level.add_tracked_entity(entity).unwrap();
        let network = level.entity::<EntityData>(entity).unwrap().network_id;
        let mut packer = ferraloom_protocol::play::BundlePacker::default();
        for (index, line) in include_str!("../../tests/java/entity/tracker/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("audience\t"))
            .enumerate()
        {
            let x = if index == 2 {
                f64::from_bits(32.0_f64.to_bits() + 1)
            } else {
                32.0
            };
            level
                .entity_mut::<EntityData>(viewer)
                .unwrap()
                .set_pos(Vec3::new(x, 100000.0, 0.0));
            if index == 3 {
                level
                    .entity_mut::<ServerPlayer>(viewer)
                    .unwrap()
                    .chunk_sender
                    .pending
                    .insert(ChunkPos::ZERO);
            }
            if index == 4 {
                level
                    .entity_mut::<ServerPlayer>(viewer)
                    .unwrap()
                    .chunk_sender
                    .pending
                    .remove(&ChunkPos::ZERO);
            }
            if index != 0 {
                level.update_tracked_player(network, viewer).unwrap();
            }
            let mut observed = Vec::new();
            for effect in level.take_effects() {
                if let LevelEffect::PlayerPacket { packet, .. } = effect
                    && let Some(message) = packer.push(*packet).unwrap()
                {
                    observed.push(match message {
                        ClientboundMessage::Packet(packet) => match *packet {
                            Packet::EntityEvent(event) => format!("event:{}", event.event_id),
                            Packet::RemoveEntities(_) => "remove".into(),
                            packet => panic!("unexpected {packet:?}"),
                        },
                        ClientboundMessage::Bundle(packets) => format!(
                            "bundle:{}",
                            packets
                                .iter()
                                .map(|packet| match packet {
                                    Packet::AddEntity(_) => "ClientboundAddEntityPacket",
                                    Packet::SetEntityData(_) => "ClientboundSetEntityDataPacket",
                                    packet => panic!("unexpected {packet:?}"),
                                })
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                    });
                }
            }
            assert_eq!(
                if observed.is_empty() {
                    "-".into()
                } else {
                    observed.join("|")
                },
                line.splitn(3, '\t').nth(2).unwrap(),
                "native audience case {index}"
            );
        }
    }
    #[test]
    fn native_entity_event_audiences_include_self_only_for_a_tracked_player() {
        let expected: Vec<_> = include_str!("../../tests/java/entity/tracker/vectors.tsv")
            .lines()
            .filter_map(|line| line.strip_prefix("event\t"))
            .map(|line| line.split_once('\t').unwrap())
            .collect();
        let (mut level, player, item) = fixture();
        let mut observed = Vec::new();
        let mut capture = |label: &str, target: EntityRef, level: &mut Level| {
            level.broadcast_entity_event(target, 3).unwrap();
            let mut deliveries = Vec::new();
            for effect in level.take_effects() {
                if let LevelEffect::PlayerPacket { recipients, packet } = effect {
                    let Packet::EntityEvent(event) = *packet else {
                        panic!("event packet")
                    };
                    assert_eq!(
                        event.entity_id,
                        level.entity::<EntityData>(target).unwrap().network_id
                    );
                    for recipient in recipients {
                        deliveries.push(format!(
                            "{}event:{}",
                            if recipient == target { "self:" } else { "" },
                            event.event_id
                        ));
                    }
                }
            }
            observed.push((
                label.to_owned(),
                if deliveries.is_empty() {
                    "-".into()
                } else {
                    deliveries.join("|")
                },
            ));
        };
        capture("missing", item, &mut level);
        level.add_tracked_entity(item).unwrap();
        level.take_effects();
        capture("item", item, &mut level);
        let viewer = level.next_entity_ref();
        let state = ServerPlayer::new(
            Uuid::new(3, 4),
            "observer".into(),
            level.next_entity_id().unwrap(),
            Vec3::new(0.0, 64.0, 0.0),
            GameType::Spectator,
        );
        level
            .add_entity(viewer, EntityState::Player(Box::new(state)))
            .unwrap();
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .remove(&ChunkPos::ZERO);
        let network = level.entity::<EntityData>(player).unwrap().network_id;
        level.update_tracked_player(network, viewer).unwrap();
        level.take_effects();
        capture("player", player, &mut level);
        level
            .entity_mut::<ServerPlayer>(viewer)
            .unwrap()
            .chunk_sender
            .pending
            .insert(ChunkPos::ZERO);
        level.update_tracked_player(network, viewer).unwrap();
        level.take_effects();
        capture("self", player, &mut level);
        assert_eq!(
            observed
                .iter()
                .map(|(label, value)| (label.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
            expected
        );
    }
    #[test]
    fn native_take_item_captures_audience_before_discard_and_preserves_gates() {
        use super::super::entity::Visibility;
        use crate::entity::{experience_orb::ExperienceOrb, item::item_entity};
        let (mut level, player, item) = fixture();
        let mut observed = Vec::new();
        let mut capture = |label: &str, level: &mut Level| {
            let mut packets = Vec::new();
            for effect in level.take_effects() {
                if let LevelEffect::PlayerPacket { recipients, packet } = effect {
                    assert_eq!(recipients, vec![player]);
                    packets.push(match *packet {
                        Packet::TakeItemEntity(packet) => format!(
                            "take:{},{},{}",
                            packet.item_id.0, packet.player_id.0, packet.amount.0
                        ),
                        Packet::EntityEvent(packet) => format!("event:{}", packet.event_id),
                        Packet::RemoveEntities(packet) => {
                            assert_eq!(packet.entity_ids, vec![VarInt(7)]);
                            "remove".into()
                        }
                        packet => panic!("unexpected pickup packet: {packet:?}"),
                    });
                }
            }
            observed.push(format!(
                "take\t{label}\t{}",
                if packets.is_empty() {
                    "-".into()
                } else {
                    packets.join("|")
                }
            ));
        };
        level.take_item(item, player, 3).unwrap();
        capture("untracked", &mut level);
        level
            .update_entity_chunk_status(ChunkPos::ZERO, Visibility::Tracked)
            .unwrap();
        level.take_effects();
        item_entity::player_touch(item, player, &mut level).unwrap();
        assert!(level.entity::<EntityData>(item).unwrap().removed);
        capture("tracked-discard", &mut level);
        // The native removed gate must suppress a packet even if an entry and
        // audience still exist in a controlled receiver frame.
        let mut entry = TrackedEntity::new(item, &level).unwrap();
        entry.seen_by.push(player);
        level.region_mut().entity_tracker.entries.insert(7, entry);
        level.take_item(item, player, 3).unwrap();
        capture("removed", &mut level);
        level.take_item(player, player, 1).unwrap();
        capture("non-item", &mut level);
        let orb = level.next_entity_ref();
        let mut state = ExperienceOrb::new(8, 1);
        state.parent.set_pos(Vec3::new(0.0, 64.0, 0.0));
        level
            .add_entity(orb, EntityState::ExperienceOrb(Box::new(state)))
            .unwrap();
        level.take_effects();
        level.take_item(orb, player, 0).unwrap();
        capture("orb-zero-count", &mut level);
        let expected: Vec<_> = include_str!("../../tests/java/entity/tracker/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("take\t"))
            .collect();
        assert_eq!(
            observed.iter().map(String::as_str).collect::<Vec<_>>(),
            expected
        );
    }
}

#[cfg(test)]
mod spectator_tests;
