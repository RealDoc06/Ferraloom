//! Region-owned canonical entity storage, vanilla ordered indexes and ticking.
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
};

use ferraloom_util::{Aabb, ChunkPos, SectionPos, Uuid, Vec3};

use super::{Level, LevelError, store::InsertOnlyMap};
use crate::entity::{EntityData, EntityRef, EntityState, RemovalReason};

mod indexes;
mod persistent;
mod publication;
use publication::EntityPublication;
mod push;
mod worldgen;
use indexes::EntityRegistration;
pub use indexes::{EntityLookup, EntitySectionStorage, EntityTickList};
pub use persistent::{
    ChunkLoadStatus, EntityChunkLoadError, EntityLoadRequestId, EntityStorageError,
    EntityStorageRequest, PersistentEntitySectionManager,
};
pub use worldgen::{WorldgenEntityLoadError, WorldgenEntityLoadStage};

#[cfg(test)]
thread_local! {
    static NEXT_ENTITY_SEED: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Control one constructor's nondeterministic RandomSource.create for native vectors.
#[cfg(test)]
pub(crate) fn with_entity_random_seed<R>(seed: i64, body: impl FnOnce() -> R) -> R {
    struct Reset(Option<i64>);
    impl Drop for Reset {
        fn drop(&mut self) {
            NEXT_ENTITY_SEED.with(|seed| seed.set(self.0));
        }
    }
    let _reset = Reset(NEXT_ENTITY_SEED.with(|next| next.replace(Some(seed))));
    body()
}

pub(crate) fn new_entity_random() -> ferraloom_util::random::LegacyRandomSource {
    #[cfg(test)]
    if let Some(seed) = NEXT_ENTITY_SEED.with(Cell::take) {
        return ferraloom_util::random::LegacyRandomSource::new(seed);
    }
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

/// Port of ServerLevel's server-wide counter, or a barrier-assigned regional block.
#[derive(Debug, Default)]
pub struct EntityIdAllocator {
    next: Cell<i32>,
    remaining: Cell<Option<u32>>,
}
impl EntityIdAllocator {
    pub fn regional(first: i32, count: u32) -> Self {
        Self {
            next: Cell::new(first.wrapping_sub(1)),
            remaining: Cell::new(Some(count)),
        }
    }
    pub fn allocate(&self, lookup: &EntityLookup) -> Option<i32> {
        loop {
            if let Some(remaining) = self.remaining.get() {
                if remaining == 0 {
                    return None;
                }
                self.remaining.set(Some(remaining - 1));
            }
            let next = self.next.get().wrapping_add(1);
            self.next.set(next);
            if next != 0 && lookup.by_id(next).is_none() {
                return Some(next);
            }
        }
    }
}

/// Canonical data is never removed while a callback frame is active.
#[derive(Default)]
pub struct EntityStorage {
    persistent: PersistentEntitySectionManager,
    registration: EntityRegistration,
    next_ref: Cell<u64>,
    effects: Vec<crate::entity::effects::EntityEffect>,
    owner_roots: HashMap<EntityRef, usize>,
    pub border: crate::level::border::WorldBorder,
    states: InsertOnlyMap<EntityRef, EntityState>,
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
    fn add(
        &self,
        namespace: super::RegionId,
        id: EntityRef,
        mut state: EntityState,
    ) -> Result<i32, LevelError> {
        if self.states.contains_key(&id) {
            return Err(LevelError::DuplicateEntity(id));
        }
        if id.allocation_namespace() == namespace {
            self.next_ref.set(self.next_ref.get().max(id.serial()));
        }
        let base = state.as_entity_mut();
        if !base.network_id_allocated {
            base.network_id = self
                .ids
                .allocate(&self.lookup)
                .expect("regional entity ID block exhausted at barrier");
            base.network_id_allocated = true;
        }
        if !self.registration.reserve_uuid(base.uuid) {
            return Err(LevelError::DuplicateEntity(id));
        }
        let network_id = base.network_id;
        let status = self.visibility(base.chunk_position);
        if (status.is_accessible() || base.entity_type == ferraloom_data::EntityType::PLAYER)
            && !self.lookup.add(base.network_id, base.uuid, id)
        {
            self.registration.forget_uuid(base.uuid);
            return Err(LevelError::DuplicateEntity(id));
        }
        self.sections.move_entity(id, base.position, status);
        self.states.publish(id, state).expect("new entity handle");
        Ok(network_id)
    }
    fn retire(&mut self, id: EntityRef) {
        self.states.remove(&id);
        self.sections.remove(id);
        self.registration.remove_unmanaged(id);
    }
    fn stop_accessing(&self, id: EntityRef) {
        if let Some(state) = self.states.get(&id) {
            self.lookup.remove(state.as_entity());
            self.sections.remove(id);
            self.registration.forget_uuid(state.as_entity().uuid);
        }
        self.registration.queue_retirement(id);
    }
    fn visibility(&self, chunk: ChunkPos) -> Visibility {
        self.persistent.visibility(chunk)
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
    /// Hold a Java entity receiver across callbacks, including after removal.
    /// Retention never restores lookup, spatial-query or tick visibility.
    pub fn retain_entity(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        self.entity_state(entity)?;
        let count = self
            .region_mut()
            .entities
            .owner_roots
            .entry(entity)
            .or_default();
        *count = count.checked_add(1).expect("entity owner roots exhausted");
        Ok(())
    }
    pub fn release_entity(&mut self, entity: EntityRef) {
        let roots = &mut self.region_mut().entities.owner_roots;
        let count = roots
            .get_mut(&entity)
            .expect("unbalanced entity root release");
        *count -= 1;
        if *count == 0 {
            roots.remove(&entity);
        }
    }
    pub fn are_entities_loaded(&self, pos: ChunkPos) -> bool {
        self.region().entities.are_entities_loaded(pos)
    }

    /// Server-lifetime handles never reuse a retired identity. Birth-Region
    /// namespaces separate equal local serials without a shared hot allocator.
    pub fn next_entity_ref(&self) -> EntityRef {
        EntityRef::allocate(self.region().id, &self.region().entities.next_ref)
    }
    /// Entity constructors share RandomSource.create's seed uniquifier with the
    /// level factory, while retaining a separate random stream per entity.
    pub fn create_entity_data(&self, kind: ferraloom_data::EntityType) -> EntityData {
        EntityData::from_random_with_id(kind, new_entity_random(), || self.next_entity_id())
    }
    pub fn emit_entity_effect(&mut self, effect: crate::entity::effects::EntityEffect) {
        self.region_mut().entities.effects.push(effect);
    }
    pub fn take_entity_effects(&mut self) -> Vec<crate::entity::effects::EntityEffect> {
        std::mem::take(&mut self.region_mut().entities.effects)
    }
    /// Port of ServerLevel#getNextEntityId. Every entity constructor, including
    /// players, draws from this region's canonical creation-order allocator.
    pub fn next_entity_id(&self) -> i32 {
        let storage = &self.region().entities;
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
        let status = self.publish_entity_data(entity, state, loaded)?;
        if status.is_accessible() {
            self.region()
                .entities
                .registration
                .push_event(EntityManagementEvent::TrackingStart(entity));
            self.add_tracked_entity(entity)
                .map_err(|error| error.into_level(self))?;
        }
        if status.is_ticking() {
            self.start_entity_ticking(entity);
        }
        Ok(())
    }
    /// Register canonical state without crossing a mutable callback boundary.
    /// Shared publication calls this only for hidden entities.
    fn publish_entity_data(
        &self,
        entity: EntityRef,
        state: EntityState,
        loaded: bool,
    ) -> Result<Visibility, LevelError> {
        let region = self.region();
        region.entities.add(region.id, entity, state)?;
        let base = region.entities.get(&entity).unwrap().as_entity();
        let status = if base.entity_type == ferraloom_data::EntityType::PLAYER {
            Visibility::Ticking
        } else {
            region.entities.visibility(base.chunk_position)
        };
        if !loaded {
            region
                .entities
                .registration
                .push_event(EntityManagementEvent::Created(entity));
        }
        Ok(status)
    }
    fn retain_unmanaged_entity(&self, entity: EntityRef, state: EntityState) {
        let storage = &self.region().entities;
        assert!(!storage.states.contains_key(&entity), "new entity handle");
        if entity.allocation_namespace() == self.region().id {
            storage
                .next_ref
                .set(storage.next_ref.get().max(entity.serial()));
        }
        storage
            .states
            .publish(entity, state)
            .expect("new entity handle");
        storage.registration.retain_unmanaged(entity);
    }
    pub fn take_entity_management_events(&self) -> Vec<EntityManagementEvent> {
        self.region().entities.registration.take_events()
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
        let storage = &self.region().entities;
        if !storage.lookup.add(network, uuid, entity) {
            return Err(LevelError::DuplicateEntity(entity));
        }
        storage
            .registration
            .push_event(EntityManagementEvent::TrackingStart(entity));
        self.add_tracked_entity(entity)
            .map_err(|error| error.into_level(self))
    }
    fn stop_entity_tracking(&mut self, entity: EntityRef) -> Result<(), LevelError> {
        self.remove_tracked_entity(entity)
            .map_err(|error| error.into_level(self))?;
        let storage = &self.region().entities;
        storage
            .registration
            .push_event(EntityManagementEvent::TrackingEnd(entity));
        storage
            .lookup
            .remove(storage.states.get(&entity).unwrap().as_entity());
        Ok(())
    }
    fn start_entity_ticking(&self, entity: EntityRef) {
        let region = self.region();
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
            .registration
            .push_event(EntityManagementEvent::TickingStart(entity));
    }
    fn stop_entity_ticking(&self, entity: EntityRef) {
        let region = self.region();
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
            .registration
            .push_event(EntityManagementEvent::TickingEnd(entity));
    }
    /// Port of PersistentEntitySectionManager#updateChunkStatus. This is driven
    /// by FullChunkStatus/ticket eligibility, independently of loaded chunks.
    pub fn update_entity_chunk_status(
        &mut self,
        pos: ChunkPos,
        status: Visibility,
    ) -> Result<(), LevelError> {
        EntityPublication::Mutable(self).update_entity_chunk_status(pos, status)
    }
    /// Shared acquisition can change visibility only when no tracker callback is required.
    pub fn update_entity_chunk_status_shared(
        &self,
        pos: ChunkPos,
        status: Visibility,
    ) -> Result<(), LevelError> {
        EntityPublication::Shared(self).update_entity_chunk_status(pos, status)
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
        if self.region().entities.registration.is_unmanaged(entity) {
            return Ok(());
        }
        if !was_removed {
            if status.is_ticking() {
                self.stop_entity_ticking(entity);
            }
            if status.is_accessible() {
                self.stop_entity_tracking(entity)?;
            }
            if reason.should_destroy() {
                self.region_mut()
                    .entities
                    .registration
                    .push_event(EntityManagementEvent::Destroyed(entity));
            }
        }
        let region = self.region_mut();
        if !was_removed {
            region.entities.stop_accessing(entity);
        }
        Ok(())
    }
    pub fn set_entity_pos(&mut self, entity: EntityRef, pos: Vec3) -> Result<(), LevelError> {
        self.set_entity_position(entity, pos, false)
    }
    /// Entity#setPosRaw publishes its section transition before reapplyPosition
    /// rebuilds the bounds, including during a live Entity#load.
    pub(crate) fn set_entity_pos_raw(
        &mut self,
        entity: EntityRef,
        pos: Vec3,
    ) -> Result<(), LevelError> {
        self.set_entity_position(entity, pos, true)
    }
    fn set_entity_position(
        &mut self,
        entity: EntityRef,
        pos: Vec3,
        raw: bool,
    ) -> Result<(), LevelError> {
        let sections = &self.region().entities.sections;
        let old_key = sections.member_section(entity);
        let previous_status = old_key
            .and_then(|key| sections.section_visibility(key))
            .unwrap_or(Visibility::Hidden);
        if raw {
            self.entity_mut::<EntityData>(entity)?.set_pos_raw(pos);
        } else {
            self.entity_mut::<EntityData>(entity)?.set_pos(pos);
        }
        if !self.entity::<EntityData>(entity)?.removed
            && !self.region().entities.registration.is_unmanaged(entity)
        {
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
                    self.stop_entity_tracking(entity)?;
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
                        .registration
                        .push_event(EntityManagementEvent::SectionChange(entity));
                }
                if self.entity::<EntityData>(entity)?.entity_type
                    == ferraloom_data::EntityType::PLAYER
                {
                    self.emit(super::effects::LevelEffect::PlayerSectionChange {
                        player: entity,
                        section,
                    });
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
            if let Some(vehicle) = base.vehicle.get() {
                if self.entity_state(vehicle).is_ok_and(|state| {
                    !state.as_entity().removed && state.as_entity().passengers.contains(&entity)
                }) {
                    continue;
                }
                crate::entity::stop_riding(entity, self)?;
            }
            self.tick_non_passenger(entity, tick_rate)?;
        }
        Ok(())
    }
    /// Port of ServerLevel#tickNonPassenger. Callbacks retain no canonical borrow.
    fn tick_non_passenger(
        &mut self,
        entity: EntityRef,
        tick_rate: &super::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        crate::entity::common_tick(entity, self)?;
        self.entity_behavior(entity)?
            .tick_with_tick_rate(entity, self, tick_rate)?;
        let passengers = self.entity::<EntityData>(entity)?.passengers.clone();
        for passenger in passengers {
            self.tick_passenger(entity, passenger, tick_rate)?;
        }
        Ok(())
    }
    fn tick_passenger(
        &mut self,
        vehicle: EntityRef,
        passenger: EntityRef,
        tick_rate: &super::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        let base = self.entity::<EntityData>(passenger)?;
        if base.removed || base.vehicle.get() != Some(vehicle) {
            crate::entity::stop_riding(passenger, self)?;
            return Ok(());
        }
        if !self.always_ticking(passenger)
            && !self.region().entity_tick_list.contains(base.network_id)
        {
            return Ok(());
        }
        crate::entity::common_tick(passenger, self)?;
        self.entity_behavior(passenger)?
            .ride_tick_with_tick_rate(passenger, self, tick_rate)?;
        let passengers = self.entity::<EntityData>(passenger)?.passengers.clone();
        for child in passengers {
            self.tick_passenger(passenger, child, tick_rate)?;
        }
        Ok(())
    }
    pub fn finish_entity_management(&mut self) {
        if !self
            .region()
            .entities
            .registration
            .has_pending_retirements()
        {
            return;
        }
        let mut reachable = HashSet::new();
        let mut work: Vec<_> = self.region().entities.owner_roots.keys().copied().collect();
        // Live registered objects and explicitly held callback receivers are
        // roots, including the references traced from a held removed receiver.
        for id in self.region().entities.sections.all_entities() {
            let state = self.entity_state(id).expect("section entity");
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
        let removed = self.region().entities.registration.take_retirements();
        for id in removed {
            if reachable.contains(&id) {
                self.region().entities.registration.queue_retirement(id);
                continue;
            }
            let region = self.region_mut();
            region.entities.retire(id);
        }
    }
}

#[cfg(test)]
mod tick_tests;

impl EntityPublication<'_> {
    pub(super) fn update_entity_chunk_status(
        &mut self,
        pos: ChunkPos,
        status: Visibility,
    ) -> Result<(), LevelError> {
        if matches!(self, EntityPublication::Shared(_)) {
            for key in self.region().entities.sections.sections_in_chunk(pos) {
                let previous = self
                    .region()
                    .entities
                    .sections
                    .section_visibility(key)
                    .unwrap();
                if previous.is_accessible() != status.is_accessible()
                    && self
                        .region()
                        .entities
                        .sections
                        .chunk_entities(pos)
                        .into_iter()
                        .any(|id| {
                            self.region().entities.sections.member_section(id) == Some(key)
                                && !self.always_ticking(id)
                        })
                {
                    return Err(LevelError::UnsupportedSharedEntityTracking(
                        "update_entity_chunk_status_shared",
                    ));
                }
            }
        }
        let storage = &self.region().entities;
        storage.persistent.update_visibility(pos, status);
        let sections = storage.sections.sections_in_chunk(pos);
        for section_key in sections {
            let (previous, entities) = self
                .region()
                .entities
                .sections
                .replace_visibility(section_key, status)
                .expect("existing section");
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
                    self.stop_tracking(id)?;
                }
            } else if !previous.is_accessible() && status.is_accessible() {
                for &id in &entities {
                    self.start_tracking(id)?;
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
}

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
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let uuid = Uuid::offline_player("JoinBot");
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
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
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
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
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                EntityState::Player(Box::new(player)),
            )
            .unwrap();
        level
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                EntityState::Cow(Box::default()),
            )
            .unwrap();
        assert_eq!(
            level
                .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2))
                .unwrap()
                .network_id,
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
    fn held_item_owner_survives_removal_and_retires_after_its_final_release() {
        use crate::entity::{
            EntityReference,
            item::ItemEntity,
            player::inventory::{count, set_count},
        };
        use ferraloom_data::Item;
        use ferraloom_protocol::component::ItemStack;

        let mut level = crate::entity::tests::level(false);
        let thrower = level.next_entity_ref();
        level
            .add_entity(thrower, EntityState::Cow(Box::default()))
            .unwrap();
        let mut item = ItemEntity::new_at(
            &mut level,
            Vec3::new(8.5, 64.0, 8.5),
            ItemStack::new(Item::STONE, 7, Default::default()),
        );
        item.thrower = Some(EntityReference::of(thrower, &level).unwrap());
        let id = level.next_entity_ref();
        level
            .add_entity(id, EntityState::Item(Box::new(item)))
            .unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        let (uuid, network, bounds) = (base.uuid, base.network_id, base.bounding_box);
        level.retain_entity(id).unwrap();
        level.retain_entity(id).unwrap();
        let (generations, _, slot) = level
            .entity_mut::<ItemEntity>(id)
            .unwrap()
            .stack_parts_mut(0)
            .unwrap();
        let held = generations.current(slot).unwrap();
        assert!(generations.retain(held));

        level
            .remove_entity_with_reason(id, RemovalReason::Discarded)
            .unwrap();
        level
            .remove_entity_with_reason(thrower, RemovalReason::Discarded)
            .unwrap();
        level.finish_entity_management();
        assert!(level.all_entities().is_empty());
        assert!(level.entities_in(bounds, |_, _| true).is_empty());
        assert_eq!(level.entity_by_uuid(uuid), None);
        assert_eq!(level.entity_by_network_id(network), None);
        assert!(!level.region().entity_tick_list.contains(network));
        let (generations, stack, _) = level
            .entity_mut::<ItemEntity>(id)
            .unwrap()
            .stack_parts_mut(0)
            .unwrap();
        set_count(generations.get_mut(held, stack).unwrap(), 3);
        assert_eq!(count(level.entity::<ItemEntity>(id).unwrap().item()), 3);
        assert!(level.entity::<EntityData>(thrower).unwrap().removed);

        level.release_entity(id);
        level.finish_entity_management();
        assert_eq!(count(level.entity::<ItemEntity>(id).unwrap().item()), 3);
        let (generations, _, _) = level
            .entity_mut::<ItemEntity>(id)
            .unwrap()
            .stack_parts_mut(0)
            .unwrap();
        assert!(generations.release(held).is_none());
        level.release_entity(id);
        level.finish_entity_management();
        assert!(
            matches!(level.entity_state(id), Err(LevelError::MissingEntity(missing)) if missing == id)
        );
        assert!(
            matches!(level.entity_state(thrower), Err(LevelError::MissingEntity(missing)) if missing == thrower)
        );
        assert_eq!(level.retain_entity(id), Err(LevelError::MissingEntity(id)));
    }

    #[test]
    fn removal_hides_indexes_before_canonical_retirement() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
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
    fn shared_publication_preserves_borrowed_canonical_state_and_rejects_replacement() {
        use crate::entity::item::PrimedTnt;
        use ferraloom_data::EntityType;

        let level = Level::new(
            Region::new(&Behaviors::EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        let make_state = || {
            EntityState::PrimedTnt(Box::new(PrimedTnt::with_parent(
                level.create_entity_data(EntityType::TNT),
            )))
        };
        let original = level.next_entity_ref();
        level
            .publish_entity_data(original, make_state(), false)
            .unwrap();
        let borrowed = level.entity::<EntityData>(original).unwrap();
        let original_uuid = borrowed.uuid;
        let passenger_view = borrowed.passengers.as_slice();
        for _ in 0..1024 {
            level
                .publish_entity_data(level.next_entity_ref(), make_state(), false)
                .unwrap();
        }
        assert_eq!(
            level.publish_entity_data(original, make_state(), false),
            Err(LevelError::DuplicateEntity(original))
        );
        assert!(std::ptr::eq(
            borrowed,
            level.entity::<EntityData>(original).unwrap()
        ));
        assert_eq!(borrowed.uuid, original_uuid);
        assert!(passenger_view.is_empty());
        assert_eq!(level.region().entities.sections.all_entities().len(), 1025);
    }

    #[test]
    fn shared_constructor_preparation_keeps_native_ids_and_borrowed_states() {
        use crate::entity::{
            persistence::load_entity_state,
            player::{GameType, ServerPlayer},
        };
        use ferraloom_data::EntityType;
        use ferraloom_nbt::{CompoundTag, Tag};

        let mut level = crate::entity::tests::level(false);
        let original = level.next_entity_ref();
        let base = level.create_entity_data(EntityType::TNT);
        level
            .add_entity(
                original,
                EntityState::PrimedTnt(Box::new(crate::entity::item::PrimedTnt::with_parent(base))),
            )
            .unwrap();
        let borrowed = level.entity::<EntityData>(original).unwrap();
        let before = borrowed.network_id;
        let mut actual = Vec::new();
        for kind in [
            EntityType::COW,
            EntityType::ITEM,
            EntityType::TNT,
            EntityType::FALLING_BLOCK,
            EntityType::EXPERIENCE_ORB,
        ] {
            let mut tag = CompoundTag::new();
            tag.insert("id", Tag::String(kind.name().into()));
            let (entity, state) = load_entity_state(&level, tag).unwrap();
            assert_eq!(entity.serial() as i32, state.as_entity().network_id);
            actual.push(format!(
                "entity-creation-order\t{}\t{}",
                kind.name(),
                state.as_entity().network_id - before
            ));
        }
        let player = ServerPlayer::new(
            Uuid::new(1, 2),
            "shared".into(),
            level.next_entity_id(),
            Vec3::ZERO,
            GameType::Survival,
        );
        assert_eq!(level.next_entity_ref().serial() as i32, player.network_id);
        actual.push(format!(
            "entity-creation-order\t{}\t{}",
            EntityType::PLAYER.name(),
            player.network_id - before
        ));
        assert!(std::ptr::eq(
            borrowed,
            level.entity::<EntityData>(original).unwrap()
        ));
        assert_eq!(borrowed.network_id, before);
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("entity-creation-order\t"))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn birth_namespaces_separate_equal_serials_and_reject_foreign_cached_reads() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let context = crate::level::LevelContext::fixture(0, 63);
        let mut first = Level::new(Region::new(&BEHAVIORS), context.clone());
        let mut second = Level::new(Region::new(&BEHAVIORS), context);
        let first_ref = first.next_entity_ref();
        let second_ref = second.next_entity_ref();
        assert_eq!(first_ref.serial(), second_ref.serial());
        assert_ne!(first_ref, second_ref);
        assert_eq!(first_ref.allocation_namespace(), first.region_id());
        assert_eq!(second_ref.allocation_namespace(), second.region_id());
        first
            .add_entity(first_ref, EntityState::Cow(Box::default()))
            .unwrap();
        second
            .add_entity(second_ref, EntityState::Cow(Box::default()))
            .unwrap();
        assert_eq!(
            second.entity_state(first_ref).unwrap_err(),
            LevelError::MissingEntity(first_ref)
        );
        assert_eq!(
            first.entity_state(second_ref).unwrap_err(),
            LevelError::MissingEntity(second_ref)
        );
        let mut cached =
            crate::entity::reference::EntityReference::<EntityData>::of(first_ref, &first).unwrap();
        assert_eq!(
            cached.resolve(&second),
            Err(LevelError::MissingEntity(first_ref))
        );
        assert_eq!(cached.resolve(&first).unwrap(), Some(first_ref));
    }

    #[test]
    fn foreign_birth_serials_do_not_advance_managed_or_retained_allocators() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let context = crate::level::LevelContext::fixture(0, 63);
        let origin = Level::new(Region::new(&BEHAVIORS), context.clone());
        let foreign = EntityRef::fixture(origin.region_id(), u64::MAX);
        let mut managed = Level::new(Region::new(&BEHAVIORS), context.clone());
        managed
            .add_entity(foreign, EntityState::Cow(Box::default()))
            .unwrap();
        assert!(managed.entity::<EntityData>(foreign).is_ok());
        let local = managed.next_entity_ref();
        assert_eq!(local.serial(), 1);
        assert_eq!(local.allocation_namespace(), managed.region_id());
        let retained = Level::new(Region::new(&BEHAVIORS), context);
        retained.retain_unmanaged_entity(foreign, EntityState::Cow(Box::default()));
        assert!(retained.entity::<EntityData>(foreign).is_ok());
        assert_eq!(retained.next_entity_ref().serial(), 1);
    }

    #[test]
    fn entity_refs_observe_existing_handles_and_never_reuse_retired_ids() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        assert_eq!(level.next_entity_ref().serial(), 1);
        let existing = EntityRef::fixture(level.region_id(), 40);
        level
            .add_entity(existing, EntityState::Cow(Box::default()))
            .unwrap();
        level.remove_entity(existing).unwrap();
        level.finish_entity_management();
        assert_eq!(level.next_entity_ref().serial(), 41);
        assert_eq!(level.next_entity_ref().serial(), 42);
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
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let pos = ChunkPos::new(0, 0);
        for n in 1..=2 {
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, n),
                    EntityState::Cow(Box::default()),
                )
                .unwrap();
        }
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 3);
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
        let uuid = level
            .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
            .unwrap()
            .uuid;
        let mut duplicate = Cow::default();
        duplicate.as_living_mut().parent.uuid = uuid;
        assert_eq!(
            level.add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 4),
                EntityState::Cow(Box::new(duplicate))
            ),
            Err(LevelError::DuplicateEntity(EntityRef::fixture(
                crate::level::RegionId::FIXTURE,
                4
            )))
        );
        level.take_entity_management_events();
        level
            .update_entity_chunk_status(pos, Visibility::Tracked)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TrackingStart(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    1
                )),
                EntityManagementEvent::TrackingStart(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    2
                ))
            ]
        );
        level
            .update_entity_chunk_status(pos, Visibility::Ticking)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TickingStart(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    1
                )),
                EntityManagementEvent::TickingStart(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    2
                ))
            ]
        );
        level
            .update_entity_chunk_status(pos, Visibility::Hidden)
            .unwrap();
        assert_eq!(
            level.take_entity_management_events(),
            [
                EntityManagementEvent::TickingEnd(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    1
                )),
                EntityManagementEvent::TickingEnd(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    2
                )),
                EntityManagementEvent::TrackingEnd(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    1
                )),
                EntityManagementEvent::TrackingEnd(EntityRef::fixture(
                    crate::level::RegionId::FIXTURE,
                    2
                ))
            ]
        );
        assert_eq!(level.all_entities(), [player]);
        assert!(level.region().entities.registration.knows_uuid(uuid));
    }
    #[test]
    fn movement_transitions_visibility_and_retirement_keeps_replacement_uuid_index() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        level
            .update_entity_chunk_status(ChunkPos::new(1, 0), Visibility::Ticking)
            .unwrap();
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
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
        let new = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
        level
            .add_entity(new, EntityState::Cow(Box::new(replacement)))
            .unwrap();
        level.finish_entity_management();
        assert_eq!(level.entity_by_uuid(uuid), Some(new));
    }
}
