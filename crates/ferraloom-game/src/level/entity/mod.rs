//! Region-owned canonical entity storage, vanilla ordered indexes and ticking.
use std::collections::{BTreeMap, HashMap, HashSet};

use ferraloom_util::{
    Aabb, ChunkPos, SectionPos, Uuid, Vec3,
    collections::{Int2ObjectLinkedOpenHashMap, Long2ObjectOpenHashMap},
};

use super::{Level, LevelError};
use crate::entity::{EntityData, EntityRef, EntityState, RemovalReason};

mod persistent;
mod worldgen;
pub use persistent::{
    ChunkLoadStatus, EntityLoadRequestId, EntityStorageError, EntityStorageRequest,
    PersistentEntitySectionManager,
};
pub use worldgen::WorldgenEntityLoadError;

pub(crate) fn new_entity_random() -> ferraloom_util::random::LegacyRandomSource {
    let ferraloom_util::random::Random::Legacy(random) = super::random::new_level_random() else {
        unreachable!("RandomSource.create uses LegacyRandomSource")
    };
    random
}

/// Port of Visibility: hidden entities remain owned but are not queryable/ticked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Visibility {
    #[default]
    Hidden,
    Tracked,
    Ticking,
}
impl Visibility {
    pub fn is_accessible(self) -> bool {
        self != Self::Hidden
    }
    pub fn is_ticking(self) -> bool {
        self == Self::Ticking
    }
}

/// Port of EntityTickList. Owned snapshots preserve membership across reentry.
#[derive(Default)]
pub struct EntityTickList {
    active: Int2ObjectLinkedOpenHashMap<EntityRef>,
}
impl EntityTickList {
    pub fn add(&mut self, id: i32, entity: EntityRef) {
        self.active.insert(id, entity);
    }
    pub fn remove(&mut self, id: i32) {
        self.active.remove(&id);
    }
    pub fn contains(&self, id: i32) -> bool {
        self.active.contains_key(&id)
    }
    pub fn snapshot(&self) -> Vec<EntityRef> {
        self.active.iter().map(|(_, id)| *id).collect()
    }
}

/// Port of EntityLookup. Only the linked runtime-ID map supplies iteration.
#[derive(Default)]
pub struct EntityLookup {
    by_id: Int2ObjectLinkedOpenHashMap<EntityRef>,
    by_uuid: HashMap<Uuid, EntityRef>,
}
impl EntityLookup {
    pub fn by_id(&self, id: i32) -> Option<EntityRef> {
        self.by_id.get(&id).copied()
    }
    pub fn by_uuid(&self, uuid: Uuid) -> Option<EntityRef> {
        self.by_uuid.get(&uuid).copied()
    }
    pub fn all(&self) -> Vec<EntityRef> {
        self.by_id.iter().map(|(_, id)| *id).collect()
    }
    fn add(&mut self, id: i32, uuid: Uuid, entity: EntityRef) -> bool {
        if self.by_uuid.contains_key(&uuid) || self.by_id.contains_key(&id) {
            return false;
        }
        self.by_id.insert(id, entity);
        self.by_uuid.insert(uuid, entity);
        true
    }
    fn remove(&mut self, base: &EntityData) {
        self.by_id.remove(&base.network_id);
        self.by_uuid.remove(&base.uuid);
    }
}
#[derive(Default)]
pub struct EntitySection {
    pub visibility: Visibility,
    entities: Vec<EntityRef>,
}

/// Port of EntitySectionStorage. Signed packed SectionPos order matches LongAVLTreeSet.
#[derive(Default)]
pub struct EntitySectionStorage {
    sections: BTreeMap<i64, EntitySection>,
    // The AVL order serves spatial queries; the original open-map key order
    // independently determines the save-chunk set's insertion history.
    section_keys: Long2ObjectOpenHashMap<()>,
    membership: HashMap<EntityRef, i64>,
}
impl EntitySectionStorage {
    fn move_entity(&mut self, entity: EntityRef, position: Vec3, visibility: Visibility) {
        let key = SectionPos::from_position(position).as_long();
        if self.membership.get(&entity) == Some(&key) {
            return;
        }
        self.remove(entity);
        self.section_keys.insert(key, ());
        self.sections
            .entry(key)
            .or_insert_with(|| EntitySection {
                visibility,
                entities: Vec::new(),
            })
            .entities
            .push(entity);
        self.membership.insert(entity, key);
    }
    fn remove(&mut self, entity: EntityRef) {
        if let Some(key) = self.membership.remove(&entity)
            && let Some(section) = self.sections.get_mut(&key)
        {
            section.entities.retain(|id| *id != entity);
            if section.entities.is_empty() {
                self.sections.remove(&key);
                self.section_keys.remove(&key);
            }
        }
    }
    pub fn query(&self, bb: Aabb) -> Vec<EntityRef> {
        let min =
            SectionPos::from_position(Vec3::new(bb.min_x - 2.0, bb.min_y - 4.0, bb.min_z - 2.0));
        let max = SectionPos::from_position(Vec3::new(bb.max_x + 2.0, bb.max_y, bb.max_z + 2.0));
        let mut result = Vec::new();
        for x in min.x..=max.x {
            let low = SectionPos::new(x, 0, 0).as_long();
            let high = SectionPos::new(x, -1, -1).as_long();
            for (&key, section) in self.sections.range(low..=high) {
                let pos = SectionPos::of(key);
                if pos.y >= min.y
                    && pos.y <= max.y
                    && pos.z >= min.z
                    && pos.z <= max.z
                    && section.visibility.is_accessible()
                {
                    result.extend(section.entities.iter().copied());
                }
            }
        }
        result
    }
}

/// Port of ServerLevel's server-wide counter, or a barrier-assigned regional block.
#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: i32,
    remaining: Option<u32>,
}
impl EntityIdAllocator {
    pub fn regional(first: i32, count: u32) -> Self {
        Self {
            next: first.wrapping_sub(1),
            remaining: Some(count),
        }
    }
    pub fn allocate(&mut self, lookup: &EntityLookup) -> Option<i32> {
        loop {
            if let Some(remaining) = &mut self.remaining {
                if *remaining == 0 {
                    return None;
                }
                *remaining -= 1;
            }
            self.next = self.next.wrapping_add(1);
            if self.next != 0 && lookup.by_id(self.next).is_none() {
                return Some(self.next);
            }
        }
    }
}

/// Canonical data is never removed while a callback frame is active.
#[derive(Default)]
pub struct EntityStorage {
    persistent: PersistentEntitySectionManager,
    known_uuids: HashSet<Uuid>,
    chunk_visibility: Long2ObjectOpenHashMap<Visibility>,
    management_events: Vec<EntityManagementEvent>,
    next_ref: u64,
    effects: Vec<crate::entity::effects::EntityEffect>,
    pending_retire: Vec<EntityRef>,
    movement_effects: Vec<crate::entity::movement::MovementEffect>,
    pub border: crate::level::border::WorldBorder,
    states: HashMap<EntityRef, EntityState>,
    pub lookup: EntityLookup,
    pub sections: EntitySectionStorage,
    pub ids: EntityIdAllocator,
}
impl EntityStorage {
    /// Port of PersistentEntitySectionManager#areEntitiesLoaded. Visibility and
    /// simulation range do not establish that entity chunk loading completed.
    pub fn are_entities_loaded(&self, pos: ChunkPos) -> bool {
        self.persistent.status(pos) == ChunkLoadStatus::Loaded
    }

    pub fn get(&self, id: &EntityRef) -> Option<&EntityState> {
        self.states.get(id)
    }
    pub fn get_mut(&mut self, id: &EntityRef) -> Option<&mut EntityState> {
        self.states.get_mut(id)
    }
    fn add(&mut self, id: EntityRef, mut state: EntityState) -> Result<i32, LevelError> {
        if self.states.contains_key(&id) {
            return Err(LevelError::DuplicateEntity(id));
        }
        self.next_ref = self.next_ref.max(id.0);
        let base = state.as_entity_mut();
        if !base.network_id_allocated {
            base.network_id = self
                .ids
                .allocate(&self.lookup)
                .expect("regional entity ID block exhausted at barrier");
            base.network_id_allocated = true;
        }
        if !self.known_uuids.insert(base.uuid) {
            return Err(LevelError::DuplicateEntity(id));
        }
        let network_id = base.network_id;
        let status = self.visibility(base.chunk_position);
        if (status.is_accessible() || base.entity_type == ferraloom_data::EntityType::PLAYER)
            && !self.lookup.add(base.network_id, base.uuid, id)
        {
            self.known_uuids.remove(&base.uuid);
            return Err(LevelError::DuplicateEntity(id));
        }
        self.sections.move_entity(id, base.position, status);
        self.states.insert(id, state);
        Ok(network_id)
    }
    fn retire(&mut self, id: EntityRef) {
        self.states.remove(&id);
        self.sections.remove(id);
    }
    fn stop_accessing(&mut self, id: EntityRef) {
        if let Some(state) = self.states.get(&id) {
            self.lookup.remove(state.as_entity());
            self.sections.remove(id);
            self.known_uuids.remove(&state.as_entity().uuid);
        }
        if !self.pending_retire.contains(&id) {
            self.pending_retire.push(id);
        }
    }
    fn visibility(&self, chunk: ChunkPos) -> Visibility {
        self.chunk_visibility
            .get(&chunk.pack())
            .copied()
            .unwrap_or(Visibility::Hidden)
    }
}

/// Port of LevelCallback notifications, retained in invocation order for the
/// server's tracking and lifecycle integrations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityManagementEvent {
    Created(EntityRef),
    Destroyed(EntityRef),
    TrackingStart(EntityRef),
    TrackingEnd(EntityRef),
    TickingStart(EntityRef),
    TickingEnd(EntityRef),
    SectionChange(EntityRef),
}

impl Level {
    pub fn are_entities_loaded(&self, pos: ChunkPos) -> bool {
        self.region().entities.are_entities_loaded(pos)
    }

    /// Server-lifetime handles never reuse a retired identity. Regional blocks
    /// are assigned by the scheduler before parallel execution is enabled.
    pub fn next_entity_ref(&mut self) -> EntityRef {
        let next = &mut self.region_mut().entities.next_ref;
        *next = next
            .checked_add(1)
            .expect("entity reference space exhausted");
        EntityRef(*next)
    }
    /// Entity constructors share RandomSource.create's seed uniquifier with the
    /// level factory, while retaining a separate random stream per entity.
    pub fn create_entity_data(&mut self, kind: ferraloom_data::EntityType) -> EntityData {
        let network_id = self.next_entity_id();
        EntityData::from_random(kind, network_id, new_entity_random())
    }
    pub fn emit_entity_effect(&mut self, effect: crate::entity::effects::EntityEffect) {
        self.region_mut().entities.effects.push(effect);
    }
    pub fn take_entity_effects(&mut self) -> Vec<crate::entity::effects::EntityEffect> {
        std::mem::take(&mut self.region_mut().entities.effects)
    }
    /// Port of ServerLevel#getNextEntityId. Every entity constructor, including
    /// players, draws from this region's canonical creation-order allocator.
    pub fn next_entity_id(&mut self) -> i32 {
        let storage = &mut self.region_mut().entities;
        storage
            .ids
            .allocate(&storage.lookup)
            .expect("regional entity ID block exhausted at barrier")
    }
    pub fn min_build_height(&self) -> i32 {
        self.region().min_y
    }
    pub fn max_build_height(&self) -> i32 {
        self.region().max_y - 1
    }

    pub fn emit_entity_movement_effect(&mut self, effect: crate::entity::movement::MovementEffect) {
        self.region_mut().entities.movement_effects.push(effect);
    }
    pub fn take_entity_movement_effects(&mut self) -> Vec<crate::entity::movement::MovementEffect> {
        std::mem::take(&mut self.region_mut().entities.movement_effects)
    }

    pub fn entity_world_border(&self) -> &crate::level::border::WorldBorder {
        &self.region().entities.border
    }
    pub fn set_entity_world_border(&mut self, border: crate::level::border::WorldBorder) {
        self.region_mut().entities.border = border;
    }
    pub fn add_entity(&mut self, entity: EntityRef, state: EntityState) -> Result<(), LevelError> {
        self.add_entity_with_origin(entity, state, false)
    }
    fn add_entity_with_origin(
        &mut self,
        entity: EntityRef,
        state: EntityState,
        loaded: bool,
    ) -> Result<(), LevelError> {
        let region = self.region_mut();
        let network_id = region.entities.add(entity, state)?;
        let base = region.entities.get(&entity).unwrap().as_entity();
        let status = if base.entity_type == ferraloom_data::EntityType::PLAYER {
            Visibility::Ticking
        } else {
            region.entities.visibility(base.chunk_position)
        };
        if !loaded {
            region
                .entities
                .management_events
                .push(EntityManagementEvent::Created(entity));
        }
        if status.is_accessible() {
            region
                .entities
                .management_events
                .push(EntityManagementEvent::TrackingStart(entity));
        }
        if status.is_ticking() {
            region.entity_tick_list.add(network_id, entity);
            region
                .entities
                .management_events
                .push(EntityManagementEvent::TickingStart(entity));
        }
        Ok(())
    }
    pub fn take_entity_management_events(&mut self) -> Vec<EntityManagementEvent> {
        std::mem::take(&mut self.region_mut().entities.management_events)
    }
    pub fn entity_chunk_visibility(&self, pos: ChunkPos) -> Visibility {
        self.region().entities.visibility(pos)
    }
    fn always_ticking(&self, entity: EntityRef) -> bool {
        self.entity::<EntityData>(entity)
            .is_ok_and(|base| base.entity_type == ferraloom_data::EntityType::PLAYER)
    }
    fn start_entity_tracking(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        let base = self.entity::<EntityData>(entity)?;
        let (network, uuid) = (base.network_id, base.uuid);
        let storage = &mut self.region_mut().entities;
        if !storage.lookup.add(network, uuid, entity) {
            return Err(LevelError::DuplicateEntity(entity));
        }
        storage
            .management_events
            .push(EntityManagementEvent::TrackingStart(entity));
        Ok(())
    }
    fn stop_entity_tracking(&mut self, entity: EntityRef) {
        let storage = &mut self.region_mut().entities;
        storage
            .management_events
            .push(EntityManagementEvent::TrackingEnd(entity));
        storage
            .lookup
            .remove(storage.states.get(&entity).unwrap().as_entity());
    }
    fn start_entity_ticking(&mut self, entity: EntityRef) {
        let region = self.region_mut();
        let id = region
            .entities
            .states
            .get(&entity)
            .unwrap()
            .as_entity()
            .network_id;
        region.entity_tick_list.add(id, entity);
        region
            .entities
            .management_events
            .push(EntityManagementEvent::TickingStart(entity));
    }
    fn stop_entity_ticking(&mut self, entity: EntityRef) {
        let region = self.region_mut();
        let id = region
            .entities
            .states
            .get(&entity)
            .unwrap()
            .as_entity()
            .network_id;
        region.entity_tick_list.remove(id);
        region
            .entities
            .management_events
            .push(EntityManagementEvent::TickingEnd(entity));
    }
    /// Port of PersistentEntitySectionManager#updateChunkStatus. This is driven
    /// by FullChunkStatus/ticket eligibility, independently of loaded chunks.
    pub fn update_entity_chunk_status(
        &mut self,
        pos: ChunkPos,
        status: Visibility,
    ) -> Result<(), LevelError> {
        let storage = &mut self.region_mut().entities;
        if status == Visibility::Hidden {
            storage.chunk_visibility.remove(&pos.pack());
            storage.persistent.chunks_to_unload.insert(pos.pack());
        } else {
            storage.chunk_visibility.insert(pos.pack(), status);
            storage.persistent.chunks_to_unload.remove(&pos.pack());
            storage.persistent.ensure_chunk_queued_for_load(pos);
        }
        let sections: Vec<_> = storage
            .sections
            .sections
            .keys()
            .copied()
            .filter(|key| {
                let section = SectionPos::of(*key);
                section.x == pos.x && section.z == pos.z
            })
            .collect();
        for section_key in sections {
            let section = self
                .region_mut()
                .entities
                .sections
                .sections
                .get_mut(&section_key)
                .unwrap();
            let previous = std::mem::replace(&mut section.visibility, status);
            let entities = section.entities.clone();
            let entities: Vec<_> = entities
                .into_iter()
                .filter(|id| !self.always_ticking(*id))
                .collect();
            if previous.is_ticking() && !status.is_ticking() {
                for &id in &entities {
                    self.stop_entity_ticking(id);
                }
            }
            if previous.is_accessible() && !status.is_accessible() {
                for &id in &entities {
                    self.stop_entity_tracking(id);
                }
            } else if !previous.is_accessible() && status.is_accessible() {
                for &id in &entities {
                    self.start_entity_tracking(id)?;
                }
            }
            if !previous.is_ticking() && status.is_ticking() {
                for &id in &entities {
                    self.start_entity_ticking(id);
                }
            }
        }
        Ok(())
    }
    pub fn remove_entity(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        self.remove_entity_with_reason(entity, RemovalReason::Discarded)
    }
    pub fn remove_entity_with_reason(
        &mut self,
        entity: EntityRef,
        reason: RemovalReason,
    ) -> Result<(), LevelError> {
        let previous = self.entity::<EntityData>(entity)?;
        let was_removed = previous.removed;
        let status = if self.always_ticking(entity) {
            Visibility::Ticking
        } else {
            self.region().entities.visibility(previous.chunk_position)
        };
        self.entity_mut::<EntityData>(entity)?.set_removed(reason);
        if self
            .entity::<EntityData>(entity)?
            .removal_reason
            .expect("removed entity reason")
            .should_destroy()
        {
            crate::entity::stop_riding(entity, self)?;
        }
        let passengers = self.entity::<EntityData>(entity)?.passengers.clone();
        for passenger in passengers {
            crate::entity::stop_riding(passenger, self)?;
        }
        if !was_removed {
            if status.is_ticking() {
                self.stop_entity_ticking(entity);
            }
            if status.is_accessible() {
                self.stop_entity_tracking(entity);
            }
            if reason.should_destroy() {
                self.region_mut()
                    .entities
                    .management_events
                    .push(EntityManagementEvent::Destroyed(entity));
            }
        }
        let region = self.region_mut();
        if !was_removed {
            region.entities.stop_accessing(entity);
        }
        Ok(())
    }
    pub fn set_entity_pos(&mut self, entity: EntityRef, pos: Vec3) -> Result<(), LevelError> {
        let old_key = self
            .region()
            .entities
            .sections
            .membership
            .get(&entity)
            .copied();
        let previous_status = old_key
            .and_then(|key| self.region().entities.sections.sections.get(&key))
            .map_or(Visibility::Hidden, |section| section.visibility);
        self.entity_mut::<EntityData>(entity)?.set_pos(pos);
        if !self.entity::<EntityData>(entity)?.removed {
            let section = SectionPos::from_position(pos);
            let new_key = section.as_long();
            let status = self
                .region()
                .entities
                .visibility(ChunkPos::new(section.x, section.z));
            self.region_mut()
                .entities
                .sections
                .move_entity(entity, pos, status);
            if old_key != Some(new_key) {
                let (before, after) = if self.always_ticking(entity) {
                    (Visibility::Ticking, Visibility::Ticking)
                } else {
                    (previous_status, status)
                };
                if before.is_accessible() && !after.is_accessible() {
                    self.stop_entity_tracking(entity);
                } else if !before.is_accessible() && after.is_accessible() {
                    self.start_entity_tracking(entity)?;
                }
                if before.is_ticking() && !after.is_ticking() {
                    self.stop_entity_ticking(entity);
                } else if !before.is_ticking() && after.is_ticking() {
                    self.start_entity_ticking(entity);
                }
                if after.is_accessible() {
                    self.region_mut()
                        .entities
                        .management_events
                        .push(EntityManagementEvent::SectionChange(entity));
                }
            }
        }
        Ok(())
    }
    pub fn entities_in(
        &self,
        bb: Aabb,
        mut predicate: impl FnMut(EntityRef, &EntityState) -> bool,
    ) -> Vec<EntityRef> {
        self.region()
            .entities
            .sections
            .query(bb)
            .into_iter()
            .filter(|id| {
                let state = self.region().entities.get(id).expect("section entity");
                state.as_entity().bounding_box.intersects(bb) && predicate(*id, state)
            })
            .collect()
    }
    pub fn entities_of_class<T: crate::entity::EntityDataView>(
        &self,
        bb: Aabb,
        mut predicate: impl FnMut(EntityRef, &T) -> bool,
    ) -> Vec<EntityRef> {
        self.entities_in(bb, |id, state| {
            T::view(state).is_some_and(|value| predicate(id, value))
        })
    }
    pub fn all_entities(&self) -> Vec<EntityRef> {
        self.region().entities.lookup.all()
    }
    pub fn entity_by_network_id(&self, id: i32) -> Option<EntityRef> {
        self.region().entities.lookup.by_id(id)
    }
    pub fn entity_by_uuid(&self, uuid: Uuid) -> Option<EntityRef> {
        self.region().entities.lookup.by_uuid(uuid)
    }
    pub fn tick_entities(&mut self) -> Result<(), LevelError> {
        self.tick_entities_with_tick_rate(&super::tick_rate::TickRateManager::default())
    }
    pub fn tick_entities_with_tick_rate(
        &mut self,
        tick_rate: &super::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        for entity in self.region().entity_tick_list.snapshot() {
            let state = self.entity_state(entity)?;
            let base = state.as_entity();
            if base.removed
                || tick_rate.is_entity_frozen(
                    state.entity_type() == ferraloom_data::EntityType::PLAYER,
                    crate::entity::count_player_passengers(entity, self)?,
                )
            {
                continue;
            }
            self.entity_behavior(entity)?.check_despawn(entity, self)?;
            let base = self.entity::<EntityData>(entity)?;
            if !self.always_ticking(entity) && !self.is_entity_ticking_chunk(base.chunk_position) {
                continue;
            }
            if let Some(vehicle) = base.vehicle {
                if self.entity_state(vehicle).is_ok_and(|state| {
                    !state.as_entity().removed && state.as_entity().passengers.contains(&entity)
                }) {
                    continue;
                }
                crate::entity::stop_riding(entity, self)?;
            }
            self.tick_non_passenger(entity)?;
        }
        Ok(())
    }
    /// Port of ServerLevel#tickNonPassenger. Callbacks retain no canonical borrow.
    fn tick_non_passenger(&mut self, mut entity: EntityRef) -> Result<(), LevelError> {
        crate::entity::common_tick(entity, self)?;
        entity.tick(self)?;
        let passengers = self.entity::<EntityData>(entity)?.passengers.clone();
        for passenger in passengers {
            self.tick_passenger(entity, passenger)?;
        }
        Ok(())
    }
    fn tick_passenger(
        &mut self,
        vehicle: EntityRef,
        mut passenger: EntityRef,
    ) -> Result<(), LevelError> {
        let base = self.entity::<EntityData>(passenger)?;
        if base.removed || base.vehicle != Some(vehicle) {
            crate::entity::stop_riding(passenger, self)?;
            return Ok(());
        }
        if !self.always_ticking(passenger)
            && !self.region().entity_tick_list.contains(base.network_id)
        {
            return Ok(());
        }
        crate::entity::common_tick(passenger, self)?;
        self.entity_mut::<EntityData>(passenger)?.velocity = Vec3::ZERO;
        passenger.tick(self)?;
        crate::entity::position_rider(vehicle, passenger, self)?;
        let passengers = self.entity::<EntityData>(passenger)?.passengers.clone();
        for child in passengers {
            self.tick_passenger(passenger, child)?;
        }
        Ok(())
    }
    pub fn finish_entity_management(&mut self) {
        if self.region().entities.pending_retire.is_empty() {
            return;
        }
        let mut reachable = HashSet::new();
        let mut work = Vec::new();
        // Map order affects only set membership, never simulation callbacks.
        for (&id, state) in &self.region().entities.states {
            if !state.as_entity().removed {
                self.entity_behavior(id)
                    .expect("stored entity dispatcher")
                    .trace_entity_refs(state, &mut work);
            }
        }
        while let Some(id) = work.pop() {
            if reachable.insert(id)
                && let Some(state) = self.region().entities.get(&id)
            {
                self.entity_behavior(id)
                    .expect("referenced entity dispatcher")
                    .trace_entity_refs(state, &mut work);
            }
        }
        let removed = std::mem::take(&mut self.region_mut().entities.pending_retire);
        for id in removed {
            if reachable.contains(&id) {
                self.region_mut().entities.pending_retire.push(id);
                continue;
            }
            let region = self.region_mut();
            region.entities.retire(id);
        }
    }
}

#[cfg(test)]
mod tick_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::player::{GameType, ServerPlayer},
        level::Region,
    };

    #[test]
    fn unassigned_network_id_allocates_one_and_retains_player_identity() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        let uuid = Uuid::offline_player("JoinBot");
        let entity = EntityRef(1);
        level
            .add_entity(
                entity,
                EntityState::Player(Box::new(ServerPlayer::new(
                    uuid,
                    "JoinBot".into(),
                    0,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(base.network_id, 1);
        assert_eq!(base.uuid, uuid);
        assert_eq!(level.entity_by_network_id(0), None);
        assert_eq!(level.entity_by_network_id(1), Some(entity));
        assert_eq!(level.entity_by_uuid(uuid), Some(entity));
        assert_eq!(
            base.save_without_id().get_int_array("UUID").unwrap(),
            uuid.to_int_array()
        );
    }

    #[test]
    fn first_player_gets_one_and_shares_allocation_with_other_entities() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        let id = level.next_entity_id();
        let player = ServerPlayer::new(
            Uuid::offline_player("JoinBot"),
            "JoinBot".into(),
            id,
            Vec3::ZERO,
            GameType::Survival,
        );
        assert_eq!(player.network_id, 1);
        level
            .add_entity(EntityRef(1), EntityState::Player(Box::new(player)))
            .unwrap();
        level
            .add_entity(EntityRef(2), EntityState::Cow(Box::default()))
            .unwrap();
        assert_eq!(
            level.entity::<EntityData>(EntityRef(2)).unwrap().network_id,
            2
        );
        assert_eq!(level.next_entity_id(), 3);
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;
    use crate::{Behaviors, level::Region};
    #[test]
    fn removal_hides_indexes_before_canonical_retirement() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        let id = EntityRef(1);
        level
            .add_entity(id, EntityState::Cow(Box::default()))
            .unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        let (uuid, network, bounds) = (base.uuid, base.network_id, base.bounding_box);
        level
            .remove_entity_with_reason(id, RemovalReason::UnloadedToChunk)
            .unwrap();
        level
            .remove_entity_with_reason(id, RemovalReason::Killed)
            .unwrap();
        assert_eq!(
            level.entity::<EntityData>(id).unwrap().removal_reason,
            Some(RemovalReason::UnloadedToChunk)
        );
        assert!(level.all_entities().is_empty());
        assert!(level.entities_in(bounds, |_, _| true).is_empty());
        assert_eq!(level.entity_by_uuid(uuid), None);
        assert_eq!(level.entity_by_network_id(network), None);
        level.tick_entities().unwrap();
        assert_eq!(level.entity::<EntityData>(id).unwrap().tick_count, 0);
        level.finish_entity_management();
        assert!(level.entity_state(id).is_err());
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::{Behaviors, level::Region};
    #[test]
    fn entity_refs_observe_existing_handles_and_never_reuse_retired_ids() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        assert_eq!(level.next_entity_ref(), EntityRef(1));
        let existing = EntityRef(40);
        level
            .add_entity(existing, EntityState::Cow(Box::default()))
            .unwrap();
        level.remove_entity(existing).unwrap();
        level.finish_entity_management();
        assert_eq!(level.next_entity_ref(), EntityRef(41));
        assert_eq!(level.next_entity_ref(), EntityRef(42));
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{
            animal::Cow,
            player::{GameType, ServerPlayer},
        },
        level::Region,
    };
    #[test]
    fn hidden_sections_keep_uuid_ownership_and_status_callbacks_keep_native_order() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        let pos = ChunkPos::new(0, 0);
        for n in 1..=2 {
            level
                .add_entity(EntityRef(n), EntityState::Cow(Box::default()))
                .unwrap();
        }
        let player = EntityRef(3);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(4, 5),
                    "fixture".into(),
                    3,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        assert_eq!(level.entity_chunk_visibility(pos), Visibility::Hidden);
        assert_eq!(level.all_entities(), [player]);
        let uuid = level.entity::<EntityData>(EntityRef(1)).unwrap().uuid;
        let mut duplicate = Cow::default();
        duplicate.as_living_mut().parent.uuid = uuid;
        assert_eq!(
            level.add_entity(EntityRef(4), EntityState::Cow(Box::new(duplicate))),
            Err(LevelError::DuplicateEntity(EntityRef(4)))
        );
        level.take_entity_management_events();
        level
            .update_entity_chunk_status(pos, Visibility::Tracked)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TrackingStart(EntityRef(1)),
                EntityManagementEvent::TrackingStart(EntityRef(2))
            ]
        );
        level
            .update_entity_chunk_status(pos, Visibility::Ticking)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TickingStart(EntityRef(1)),
                EntityManagementEvent::TickingStart(EntityRef(2))
            ]
        );
        level
            .update_entity_chunk_status(pos, Visibility::Hidden)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TickingEnd(EntityRef(1)),
                EntityManagementEvent::TickingEnd(EntityRef(2)),
                EntityManagementEvent::TrackingEnd(EntityRef(1)),
                EntityManagementEvent::TrackingEnd(EntityRef(2))
            ]
        );
        assert_eq!(level.all_entities(), [player]);
        assert!(level.region().entities.known_uuids.contains(&uuid));
    }
    #[test]
    fn movement_transitions_visibility_and_retirement_keeps_replacement_uuid_index() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS));
        level
            .update_entity_chunk_status(ChunkPos::new(1, 0), Visibility::Ticking)
            .unwrap();
        let id = EntityRef(1);
        level
            .add_entity(id, EntityState::Cow(Box::default()))
            .unwrap();
        level
            .set_entity_pos(id, Vec3::new(16.5, 64.0, 0.5))
            .unwrap();
        assert_eq!(level.all_entities(), [id]);
        let uuid = level.entity::<EntityData>(id).unwrap().uuid;
        level.remove_entity(id).unwrap();
        let mut replacement = Cow::default();
        replacement.as_living_mut().parent.uuid = uuid;
        replacement.as_living_mut().parent.network_id = level.next_entity_id();
        replacement.as_living_mut().parent.network_id_allocated = true;
        replacement
            .as_living_mut()
            .parent
            .set_pos(Vec3::new(16.5, 64.0, 0.5));
        let new = EntityRef(2);
        level
            .add_entity(new, EntityState::Cow(Box::new(replacement)))
            .unwrap();
        level.finish_entity_management();
        assert_eq!(level.entity_by_uuid(uuid), Some(new));
    }
}
