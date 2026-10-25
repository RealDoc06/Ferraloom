//! Index guards stay in this module; world calls receive owned handle lists.
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet},
};

use super::{EntityManagementEvent, Visibility};
use crate::entity::{EntityData, EntityRef};
use ferraloom_util::{
    Aabb, ChunkPos, SectionPos, Uuid, Vec3,
    collections::{Int2ObjectLinkedOpenHashMap, Long2ObjectOpenHashMap},
};

/// Port of EntityTickList. Snapshots preserve membership across reentry.
/// Phase-5 region ownership replaces this single-region interior index access.
#[derive(Default)]
pub struct EntityTickList {
    active: RefCell<Int2ObjectLinkedOpenHashMap<EntityRef>>,
}
impl EntityTickList {
    pub fn add(&self, id: i32, entity: EntityRef) {
        self.active.borrow_mut().insert(id, entity);
    }
    pub fn remove(&self, id: i32) {
        self.active.borrow_mut().remove(&id);
    }
    pub fn contains(&self, id: i32) -> bool {
        self.active.borrow().contains_key(&id)
    }
    pub fn snapshot(&self) -> Vec<EntityRef> {
        self.active.borrow().iter().map(|(_, id)| *id).collect()
    }
}

#[derive(Default)]
struct LookupIndexes {
    by_id: Int2ObjectLinkedOpenHashMap<EntityRef>,
    by_uuid: HashMap<Uuid, EntityRef>,
}

/// Port of EntityLookup. Only the linked runtime-ID map supplies iteration.
/// Phase-5 region ownership replaces this single-region interior index access.
#[derive(Default)]
pub struct EntityLookup {
    indexes: RefCell<LookupIndexes>,
}
impl EntityLookup {
    pub fn by_id(&self, id: i32) -> Option<EntityRef> {
        self.indexes.borrow().by_id.get(&id).copied()
    }
    pub fn by_uuid(&self, uuid: Uuid) -> Option<EntityRef> {
        self.indexes.borrow().by_uuid.get(&uuid).copied()
    }
    pub fn all(&self) -> Vec<EntityRef> {
        self.indexes
            .borrow()
            .by_id
            .iter()
            .map(|(_, id)| *id)
            .collect()
    }
    pub(super) fn add(&self, id: i32, uuid: Uuid, entity: EntityRef) -> bool {
        let mut indexes = self.indexes.borrow_mut();
        if indexes.by_uuid.contains_key(&uuid) || indexes.by_id.contains_key(&id) {
            return false;
        }
        indexes.by_id.insert(id, entity);
        indexes.by_uuid.insert(uuid, entity);
        true
    }
    pub(super) fn remove(&self, base: &EntityData) {
        let mut indexes = self.indexes.borrow_mut();
        indexes.by_id.remove(&base.network_id);
        indexes.by_uuid.remove(&base.uuid);
    }
}

struct EntitySection {
    visibility: Visibility,
    entities: Vec<EntityRef>,
}
#[derive(Default)]
struct SectionIndexes {
    sections: BTreeMap<i64, EntitySection>,
    // AVL order serves spatial queries; open-map key order independently
    // determines the save-chunk set's insertion history.
    section_keys: Long2ObjectOpenHashMap<()>,
    membership: HashMap<EntityRef, i64>,
}
impl SectionIndexes {
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
}

/// Port of EntitySectionStorage; signed packed order matches LongAVLTreeSet.
/// Phase-5 region ownership replaces this single-region interior index access.
#[derive(Default)]
pub struct EntitySectionStorage {
    indexes: RefCell<SectionIndexes>,
}
impl EntitySectionStorage {
    pub(super) fn move_entity(&self, entity: EntityRef, position: Vec3, visibility: Visibility) {
        let key = SectionPos::from_position(position).as_long();
        let mut indexes = self.indexes.borrow_mut();
        if indexes.membership.get(&entity) == Some(&key) {
            return;
        }
        indexes.remove(entity);
        indexes.section_keys.insert(key, ());
        indexes
            .sections
            .entry(key)
            .or_insert_with(|| EntitySection {
                visibility,
                entities: Vec::new(),
            })
            .entities
            .push(entity);
        indexes.membership.insert(entity, key);
    }
    pub(super) fn remove(&self, entity: EntityRef) {
        self.indexes.borrow_mut().remove(entity);
    }
    pub(super) fn member_section(&self, entity: EntityRef) -> Option<i64> {
        self.indexes.borrow().membership.get(&entity).copied()
    }
    pub(super) fn section_visibility(&self, key: i64) -> Option<Visibility> {
        self.indexes
            .borrow()
            .sections
            .get(&key)
            .map(|section| section.visibility)
    }
    pub(super) fn sections_in_chunk(&self, pos: ChunkPos) -> Vec<i64> {
        let low = SectionPos::new(pos.x, 0, pos.z).as_long();
        let high = SectionPos::new(pos.x, -1, pos.z).as_long();
        self.indexes
            .borrow()
            .sections
            .range(low..=high)
            .map(|(&key, _)| key)
            .collect()
    }
    pub(super) fn replace_visibility(
        &self,
        key: i64,
        visibility: Visibility,
    ) -> Option<(Visibility, Vec<EntityRef>)> {
        let mut indexes = self.indexes.borrow_mut();
        let section = indexes.sections.get_mut(&key)?;
        let previous = std::mem::replace(&mut section.visibility, visibility);
        Some((previous, section.entities.clone()))
    }
    pub(super) fn chunk_entities(&self, pos: ChunkPos) -> Vec<EntityRef> {
        let low = SectionPos::new(pos.x, 0, pos.z).as_long();
        let high = SectionPos::new(pos.x, -1, pos.z).as_long();
        self.indexes
            .borrow()
            .sections
            .range(low..=high)
            .flat_map(|(_, section)| section.entities.iter().copied())
            .collect()
    }
    pub(super) fn all_entities(&self) -> Vec<EntityRef> {
        self.indexes
            .borrow()
            .sections
            .values()
            .flat_map(|section| section.entities.iter().copied())
            .collect()
    }
    pub(super) fn section_keys(&self) -> Vec<i64> {
        self.indexes
            .borrow()
            .section_keys
            .iter()
            .map(|(&key, _)| key)
            .collect()
    }
    pub fn query(&self, bb: Aabb) -> Vec<EntityRef> {
        let min =
            SectionPos::from_position(Vec3::new(bb.min_x - 2.0, bb.min_y - 4.0, bb.min_z - 2.0));
        let max = SectionPos::from_position(Vec3::new(bb.max_x + 2.0, bb.max_y, bb.max_z + 2.0));
        let indexes = self.indexes.borrow();
        let mut result = Vec::new();
        for x in min.x..=max.x {
            let low = SectionPos::new(x, 0, 0).as_long();
            let high = SectionPos::new(x, -1, -1).as_long();
            for (&key, section) in indexes.sections.range(low..=high) {
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

#[derive(Default)]
struct RegistrationIndexes {
    known_uuids: HashSet<Uuid>,
    // Rejected duplicate objects have a NULL vanilla level callback but may
    // remain reachable through successfully registered passengers.
    unmanaged: HashSet<EntityRef>,
    pending_retire: Vec<EntityRef>,
    events: Vec<EntityManagementEvent>,
}

/// UUID ownership and lifecycle queues, disjoint from canonical entity state.
/// Phase-5 region ownership replaces this single-region interior index access.
#[derive(Default)]
pub(super) struct EntityRegistration {
    indexes: RefCell<RegistrationIndexes>,
}
impl EntityRegistration {
    pub(super) fn knows_uuid(&self, uuid: Uuid) -> bool {
        self.indexes.borrow().known_uuids.contains(&uuid)
    }
    pub(super) fn reserve_uuid(&self, uuid: Uuid) -> bool {
        self.indexes.borrow_mut().known_uuids.insert(uuid)
    }
    pub(super) fn forget_uuid(&self, uuid: Uuid) {
        self.indexes.borrow_mut().known_uuids.remove(&uuid);
    }
    pub(super) fn is_unmanaged(&self, entity: EntityRef) -> bool {
        self.indexes.borrow().unmanaged.contains(&entity)
    }
    pub(super) fn retain_unmanaged(&self, entity: EntityRef) {
        let mut indexes = self.indexes.borrow_mut();
        indexes.unmanaged.insert(entity);
        indexes.pending_retire.push(entity);
    }
    pub(super) fn remove_unmanaged(&self, entity: EntityRef) {
        self.indexes.borrow_mut().unmanaged.remove(&entity);
    }
    pub(super) fn publish_unmanaged(&self, entity: EntityRef) {
        let mut indexes = self.indexes.borrow_mut();
        indexes.unmanaged.remove(&entity);
        indexes.pending_retire.retain(|pending| *pending != entity);
    }
    pub(super) fn has_pending_retirements(&self) -> bool {
        !self.indexes.borrow().pending_retire.is_empty()
    }
    pub(super) fn queue_retirement(&self, entity: EntityRef) {
        let mut indexes = self.indexes.borrow_mut();
        if !indexes.pending_retire.contains(&entity) {
            indexes.pending_retire.push(entity);
        }
    }
    pub(super) fn take_retirements(&self) -> Vec<EntityRef> {
        std::mem::take(&mut self.indexes.borrow_mut().pending_retire)
    }
    pub(super) fn push_event(&self, event: EntityManagementEvent) {
        self.indexes.borrow_mut().events.push(event);
    }
    pub(super) fn take_events(&self) -> Vec<EntityManagementEvent> {
        std::mem::take(&mut self.indexes.borrow_mut().events)
    }
    #[cfg(test)]
    pub(super) fn uuid_count(&self) -> usize {
        self.indexes.borrow().known_uuids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_index_results_allow_publication_before_iteration_resumes() {
        let a = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let b = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
        let ticks = EntityTickList::default();
        ticks.add(10, a);
        let tick_snapshot = ticks.snapshot();
        ticks.remove(10);
        ticks.add(11, b);
        assert_eq!(tick_snapshot, [a]);
        assert_eq!(ticks.snapshot(), [b]);

        let lookup = EntityLookup::default();
        assert!(lookup.add(10, Uuid::new(1, 1), a));
        let visible_snapshot = lookup.all();
        assert!(lookup.add(11, Uuid::new(1, 2), b));
        assert_eq!(visible_snapshot, [a]);
        assert_eq!(lookup.all(), [a, b]);

        let sections = EntitySectionStorage::default();
        sections.move_entity(a, Vec3::ZERO, Visibility::Ticking);
        let bb = Aabb::new(-1.0, -1.0, -1.0, 2.0, 2.0, 2.0);
        let query_snapshot = sections.query(bb);
        sections.move_entity(b, Vec3::ZERO, Visibility::Ticking);
        assert_eq!(query_snapshot, [a]);
        assert_eq!(sections.query(bb), [a, b]);
    }
}
