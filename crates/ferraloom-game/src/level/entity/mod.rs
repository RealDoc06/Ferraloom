//! Region-owned canonical entity storage, vanilla ordered indexes and ticking.
use std::collections::{BTreeMap, HashMap};

use ferraloom_util::{
    Aabb, ChunkPos, SectionPos, Uuid, Vec3, collections::Int2ObjectLinkedOpenHashMap,
};

use super::{Level, LevelError};
use crate::entity::{EntityData, EntityRef, EntityState, RemovalReason};

/// Port of Visibility: hidden entities remain owned but are not queryable/ticked.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Visibility {
    Hidden,
    Tracked,
    #[default]
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
    membership: HashMap<EntityRef, i64>,
}
impl EntitySectionStorage {
    fn move_entity(&mut self, entity: EntityRef, position: Vec3) {
        let key = SectionPos::from_position(position).as_long();
        if self.membership.get(&entity) == Some(&key) {
            return;
        }
        self.remove(entity);
        self.sections.entry(key).or_default().entities.push(entity);
        self.membership.insert(entity, key);
    }
    fn remove(&mut self, entity: EntityRef) {
        if let Some(key) = self.membership.remove(&entity)
            && let Some(section) = self.sections.get_mut(&key)
        {
            section.entities.retain(|id| *id != entity);
            if section.entities.is_empty() {
                self.sections.remove(&key);
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
    pub fn set_visibility(&mut self, chunk: ChunkPos, visibility: Visibility) {
        for (&key, section) in &mut self.sections {
            let pos = SectionPos::of(key);
            if pos.x == chunk.x && pos.z == chunk.z {
                section.visibility = visibility;
            }
        }
    }
}

/// Port of ServerLevel's server-wide counter, or a barrier-assigned regional block.
#[derive(Debug)]
pub struct EntityIdAllocator {
    next: i32,
    remaining: Option<u32>,
}
impl Default for EntityIdAllocator {
    fn default() -> Self {
        Self {
            next: 0,
            remaining: None,
        }
    }
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
    movement_effects: Vec<crate::entity::movement::MovementEffect>,
    pub border: crate::level::border::WorldBorder,
    states: HashMap<EntityRef, EntityState>,
    pub lookup: EntityLookup,
    pub sections: EntitySectionStorage,
    pub ids: EntityIdAllocator,
}
impl EntityStorage {
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
        let base = state.as_entity_mut();
        if base.network_id == 0 {
            base.network_id = self
                .ids
                .allocate(&self.lookup)
                .expect("regional entity ID block exhausted at barrier");
            // Constructors without an allocated ID are foundation callers. Use a
            // distinct stream until the server supplies its entity seed allocator.
            base.random = ferraloom_util::random::LegacyRandomSource::new(id.0 as i64);
            base.uuid = Uuid::insecure(&mut base.random);
        }
        if !self.lookup.add(base.network_id, base.uuid, id) {
            return Err(LevelError::DuplicateEntity(id));
        }
        let network_id = base.network_id;
        self.sections.move_entity(id, base.position);
        self.states.insert(id, state);
        Ok(network_id)
    }
    fn retire(&mut self, id: EntityRef) {
        if let Some(state) = self.states.remove(&id) {
            self.lookup.remove(state.as_entity());
            self.sections.remove(id);
        }
    }
}

impl Level {
    pub fn min_build_height(&self) -> i32 {
        self.region().min_y
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
        let region = self.region_mut();
        let network_id = region.entities.add(entity, state)?;
        region.entity_tick_list.add(network_id, entity);
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
        self.entity_mut::<EntityData>(entity)?.set_removed(reason);
        if reason.should_destroy() {
            crate::entity::stop_riding(entity, self)?;
        }
        crate::entity::eject_passengers(entity, self)?;
        Ok(())
    }
    pub fn set_entity_pos(&mut self, entity: EntityRef, pos: Vec3) -> Result<(), LevelError> {
        self.entity_mut::<EntityData>(entity)?.set_pos(pos);
        self.region_mut().entities.sections.move_entity(entity, pos);
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
        let removed: Vec<_> = self
            .all_entities()
            .into_iter()
            .filter(|id| self.region().entities.get(id).unwrap().as_entity().removed)
            .collect();
        for id in removed {
            let network_id = self
                .region()
                .entities
                .get(&id)
                .unwrap()
                .as_entity()
                .network_id;
            let region = self.region_mut();
            region.entity_tick_list.remove(network_id);
            region.entities.retire(id);
        }
    }
}
