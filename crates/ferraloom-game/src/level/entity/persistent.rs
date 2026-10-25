//! PersistentEntitySectionManager's state machine. The server executes the
//! owned requests on its IO worker and returns results at the management phase.
use super::EntityPublication;
use std::{cell::RefCell, collections::VecDeque};

use super::Visibility;
use crate::{
    entity::{
        EntityData, EntityRef, RemovalReason,
        persistence::{self, EntityPersistenceError},
    },
    level::{Level, LevelError},
};
use ferraloom_nbt::CompoundTag;
use ferraloom_storage::{EntityChunk, IntArray};
use ferraloom_util::{
    ChunkPos, SectionPos,
    collections::{Long2ObjectOpenHashMap, LongOpenHashSet},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ChunkLoadStatus {
    #[default]
    Fresh,
    Pending,
    Loaded,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityLoadRequestId(pub u64);

/// Region requests contain owned snapshots, never references into live state.
#[derive(Debug)]
pub enum EntityStorageRequest {
    Load {
        id: EntityLoadRequestId,
        pos: ChunkPos,
    },
    Store {
        pos: ChunkPos,
        chunk: Option<EntityChunk>,
    },
}
#[derive(Debug)]
pub enum EntityStorageError {
    LoadIntegration(Box<EntityChunkLoadError>),
    Level(LevelError),
    Entity(EntityPersistenceError),
    Storage(ferraloom_storage::Error),
    UnknownLoadRequest(EntityLoadRequestId),
    DuplicateLoadResponse(EntityLoadRequestId),
    MisplacedChunk {
        expected: ChunkPos,
        stored: ChunkPos,
    },
    SaveAlreadyInProgress,
}

/// Failed entity IO integration keeps its original owned chunk NBT. Preflight
/// consumed no constructor draws; Publishing must never be replayed from NBT.
#[derive(Debug)]
pub struct EntityChunkLoadError {
    pub id: EntityLoadRequestId,
    pub pos: ChunkPos,
    pub chunk: Option<CompoundTag>,
    pub source: EntityStorageError,
    pub stage: super::WorldgenEntityLoadStage,
}
impl From<LevelError> for EntityStorageError {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
impl From<EntityPersistenceError> for EntityStorageError {
    fn from(error: EntityPersistenceError) -> Self {
        Self::Entity(error)
    }
}
impl From<ferraloom_storage::Error> for EntityStorageError {
    fn from(error: ferraloom_storage::Error) -> Self {
        Self::Storage(error)
    }
}
impl std::fmt::Display for EntityStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EntityStorageError {}

struct PendingLoad {
    id: EntityLoadRequestId,
    pos: ChunkPos,
    response: Option<ferraloom_storage::Result<Option<CompoundTag>>>,
}

/// Port of PersistentEntitySectionManager's IO/visibility indexes. No guard
/// leaves this module or spans construction, registration or a world callback.
/// Phase-5 region ownership replaces this single-region interior index access.
#[derive(Default)]
pub struct PersistentEntitySectionManager {
    state: RefCell<PersistentState>,
}
#[derive(Default)]
struct PersistentState {
    chunk_visibility: Long2ObjectOpenHashMap<Visibility>,
    load_statuses: Long2ObjectOpenHashMap<ChunkLoadStatus>,
    chunks_to_unload: LongOpenHashSet,
    empty_chunks: LongOpenHashSet,
    chunk_remainders: Long2ObjectOpenHashMap<CompoundTag>,
    saving_all: Option<LongOpenHashSet>,
    next_request: u64,
    loads: VecDeque<PendingLoad>,
    requests: Vec<EntityStorageRequest>,
}
impl PersistentState {
    fn status(&self, pos: ChunkPos) -> ChunkLoadStatus {
        self.load_statuses
            .get(&pos.pack())
            .copied()
            .unwrap_or_default()
    }
    fn ensure_chunk_queued_for_load(&mut self, pos: ChunkPos) {
        if self.status(pos) != ChunkLoadStatus::Fresh {
            return;
        }
        self.next_request = self
            .next_request
            .checked_add(1)
            .expect("entity IO ordinal exhausted");
        let id = EntityLoadRequestId(self.next_request);
        self.load_statuses
            .insert(pos.pack(), ChunkLoadStatus::Pending);
        let empty = self.empty_chunks.contains(&pos.pack());
        self.loads.push_back(PendingLoad {
            id,
            pos,
            response: empty.then_some(Ok(None)),
        });
        if !empty {
            self.requests.push(EntityStorageRequest::Load { id, pos });
        }
    }
    fn store(&mut self, pos: ChunkPos, entities: Vec<CompoundTag>) {
        if entities.is_empty() {
            if self.empty_chunks.insert(pos.pack()) {
                self.requests
                    .push(EntityStorageRequest::Store { pos, chunk: None });
            }
        } else {
            self.empty_chunks.remove(&pos.pack());
            let chunk = EntityChunk {
                data_version: Some(ferraloom_storage::DATA_VERSION),
                position: Some(IntArray(vec![pos.x, pos.z])),
                entities: Some(entities),
                extra: self
                    .chunk_remainders
                    .get(&pos.pack())
                    .cloned()
                    .unwrap_or_default(),
            };
            self.requests.push(EntityStorageRequest::Store {
                pos,
                chunk: Some(chunk),
            });
        }
    }
}

impl PersistentEntitySectionManager {
    pub(super) fn status(&self, pos: ChunkPos) -> ChunkLoadStatus {
        self.state.borrow().status(pos)
    }
    pub(super) fn visibility(&self, pos: ChunkPos) -> Visibility {
        self.state
            .borrow()
            .chunk_visibility
            .get(&pos.pack())
            .copied()
            .unwrap_or(Visibility::Hidden)
    }
    pub(super) fn update_visibility(&self, pos: ChunkPos, status: Visibility) {
        let mut state = self.state.borrow_mut();
        if status == Visibility::Hidden {
            state.chunk_visibility.remove(&pos.pack());
            state.chunks_to_unload.insert(pos.pack());
        } else {
            state.chunk_visibility.insert(pos.pack(), status);
            state.chunks_to_unload.remove(&pos.pack());
            state.ensure_chunk_queued_for_load(pos);
        }
    }
}

impl Level {
    pub fn entity_chunk_load_status(&self, pos: ChunkPos) -> ChunkLoadStatus {
        self.region().entities.persistent.status(pos)
    }
    pub fn take_entity_storage_requests(&self) -> Vec<EntityStorageRequest> {
        std::mem::take(
            &mut self
                .region()
                .entities
                .persistent
                .state
                .borrow_mut()
                .requests,
        )
    }
    /// IO responses may arrive in any order. Only process_pending_entity_loads
    /// makes them visible, in original request order, at the management phase.
    pub fn complete_entity_chunk_load(
        &self,
        id: EntityLoadRequestId,
        response: ferraloom_storage::Result<Option<CompoundTag>>,
    ) -> Result<(), EntityStorageError> {
        let mut state = self.region().entities.persistent.state.borrow_mut();
        let load = state
            .loads
            .iter_mut()
            .find(|load| load.id == id)
            .ok_or(EntityStorageError::UnknownLoadRequest(id))?;
        if load.response.is_some() {
            return Err(EntityStorageError::DuplicateLoadResponse(id));
        }
        load.response = Some(response);
        Ok(())
    }
    /// Port of PersistentEntitySectionManager#processPendingLoads. Explicit
    /// decode/unsupported errors leave the chunk pending and prevent overwrites.
    pub fn process_pending_entity_loads(&mut self) -> Result<(), EntityStorageError> {
        EntityPublication::Mutable(self).publish_pending_entity_loads()
    }
    /// Shared acquisition can publish hidden entities; tracking requires the
    /// pipeline's main-thread mutable continuation and otherwise fails explicitly.
    pub fn process_pending_entity_loads_shared(&self) -> Result<(), EntityStorageError> {
        EntityPublication::Shared(self).publish_pending_entity_loads()
    }
    fn entity_chunk_roots_to_save(&self, pos: ChunkPos) -> Result<Vec<EntityRef>, LevelError> {
        let mut roots = Vec::new();
        for id in self.region().entities.sections.chunk_entities(pos) {
            if self.entity_should_be_saved(id)? {
                roots.push(id);
            }
        }
        Ok(roots)
    }
    /// False means pending IO is required before this chunk may be overwritten.
    fn store_entity_chunk_sections(
        &mut self,
        pos: ChunkPos,
        unload: bool,
    ) -> Result<bool, EntityStorageError> {
        let status = self.entity_chunk_load_status(pos);
        if status == ChunkLoadStatus::Pending {
            return Ok(false);
        }
        let roots = self.entity_chunk_roots_to_save(pos)?;
        if roots.is_empty() {
            if status == ChunkLoadStatus::Loaded {
                self.region_mut()
                    .entities
                    .persistent
                    .state
                    .get_mut()
                    .store(pos, Vec::new());
            }
        } else if status == ChunkLoadStatus::Fresh {
            self.region_mut()
                .entities
                .persistent
                .state
                .get_mut()
                .ensure_chunk_queued_for_load(pos);
            return Ok(false);
        } else {
            let mut tags = Vec::new();
            for &root in &roots {
                if let Some(tag) = self.save_entity(root)? {
                    tags.push(tag);
                }
            }
            self.region_mut()
                .entities
                .persistent
                .state
                .get_mut()
                .store(pos, tags);
            if unload {
                for root in roots {
                    let mut unload_order = Vec::new();
                    self.entity_passengers_and_self(root, &mut unload_order)?;
                    for id in unload_order {
                        self.remove_entity_with_reason(id, RemovalReason::UnloadedToChunk)?;
                    }
                }
            }
        }
        if unload {
            self.region_mut()
                .entities
                .persistent
                .state
                .get_mut()
                .load_statuses
                .remove(&pos.pack());
        }
        Ok(true)
    }
    // Entity#getPassengersAndSelf emits descendants before their root.
    fn entity_passengers_and_self(
        &self,
        root: EntityRef,
        out: &mut Vec<EntityRef>,
    ) -> Result<(), LevelError> {
        for &passenger in &self.entity::<EntityData>(root)?.passengers {
            self.entity_passengers_and_self(passenger, out)?;
        }
        out.push(root);
        Ok(())
    }
    /// Port of PersistentEntitySectionManager#tick; run after the entity and
    /// block-entity phases, including when normal simulation is frozen.
    pub fn tick_entity_management(&mut self) -> Result<(), EntityStorageError> {
        self.process_pending_entity_loads()?;
        let chunks: Vec<_> = self
            .region()
            .entities
            .persistent
            .state
            .borrow()
            .chunks_to_unload
            .iter()
            .copied()
            .collect();
        let mut completed = LongOpenHashSet::new();
        for key in chunks {
            let pos = ChunkPos::unpack(key);
            if self.entity_chunk_visibility(pos) != Visibility::Hidden
                || self.store_entity_chunk_sections(pos, true)?
            {
                completed.insert(key);
            }
        }
        let mut cursor = self
            .region_mut()
            .entities
            .persistent
            .state
            .get_mut()
            .chunks_to_unload
            .cursor();
        while let Some((&key, _)) = cursor.next_entry() {
            if completed.contains(&key) {
                cursor.remove();
            }
        }
        self.finish_entity_management();
        Ok(())
    }
    fn all_entity_chunks_to_save(&self) -> LongOpenHashSet {
        let mut chunks = LongOpenHashSet::new();
        for section in self.region().entities.sections.section_keys() {
            let section = SectionPos::of(section);
            chunks.insert(ChunkPos::new(section.x, section.z).pack());
        }
        for (&key, &status) in &self
            .region()
            .entities
            .persistent
            .state
            .borrow()
            .load_statuses
        {
            if status == ChunkLoadStatus::Loaded {
                chunks.insert(key);
            }
        }
        chunks
    }
    /// Port of PersistentEntitySectionManager#autoSave. Save capture is ordered;
    /// the server must submit every returned request before requesting IO flush.
    pub fn auto_save_entities(&mut self) -> Result<(), EntityStorageError> {
        let chunks = self.all_entity_chunks_to_save();
        for &key in chunks.iter() {
            let pos = ChunkPos::unpack(key);
            self.store_entity_chunk_sections(
                pos,
                self.entity_chunk_visibility(pos) == Visibility::Hidden,
            )?;
        }
        Ok(())
    }
    /// Start saveAll's fixed work set. The server submits requests and waits for
    /// IoWorker::synchronize(false) before each subsequent poll, then performs
    /// synchronize(true) once poll_entity_save_all returns true.
    pub fn begin_entity_save_all(&mut self) -> Result<(), EntityStorageError> {
        if self
            .region()
            .entities
            .persistent
            .state
            .borrow()
            .saving_all
            .is_some()
        {
            return Err(EntityStorageError::SaveAlreadyInProgress);
        }
        let chunks = self.all_entity_chunks_to_save();
        self.region_mut()
            .entities
            .persistent
            .state
            .get_mut()
            .saving_all = Some(chunks);
        Ok(())
    }
    pub fn poll_entity_save_all(&mut self) -> Result<bool, EntityStorageError> {
        self.process_pending_entity_loads()?;
        let keys: Vec<_> = self
            .region()
            .entities
            .persistent
            .state
            .borrow()
            .saving_all
            .as_ref()
            .map(|chunks| chunks.iter().copied().collect())
            .unwrap_or_default();
        let mut completed = LongOpenHashSet::new();
        for key in keys {
            let pos = ChunkPos::unpack(key);
            if self.store_entity_chunk_sections(
                pos,
                self.entity_chunk_visibility(pos) == Visibility::Hidden,
            )? {
                completed.insert(key);
            }
        }
        let manager = self.region_mut().entities.persistent.state.get_mut();
        let Some(chunks) = manager.saving_all.as_mut() else {
            return Ok(true);
        };
        let mut cursor = chunks.cursor();
        while let Some((&key, _)) = cursor.next_entry() {
            if completed.contains(&key) {
                cursor.remove();
            }
        }
        if chunks.is_empty() {
            manager.saving_all = None;
            return Ok(true);
        }
        Ok(false)
    }
}

impl EntityPublication<'_> {
    fn publish_pending_entity_loads(&mut self) -> Result<(), EntityStorageError> {
        loop {
            let load = {
                let mut manager = self.region().entities.persistent.state.borrow_mut();
                if !manager
                    .loads
                    .front()
                    .is_some_and(|load| load.response.is_some())
                {
                    break;
                }
                manager.loads.pop_front().unwrap()
            };
            let tag = load.response.unwrap().map_err(|source| {
                EntityStorageError::LoadIntegration(Box::new(EntityChunkLoadError {
                    id: load.id,
                    pos: load.pos,
                    chunk: None,
                    source: source.into(),
                    stage: super::WorldgenEntityLoadStage::Preflight,
                }))
            })?;
            let mut stage = super::WorldgenEntityLoadStage::Preflight;
            if let Err(source) =
                self.integrate_entity_chunk_load(load.pos, tag.as_ref(), &mut stage)
            {
                return Err(EntityStorageError::LoadIntegration(Box::new(
                    EntityChunkLoadError {
                        id: load.id,
                        pos: load.pos,
                        chunk: tag,
                        source,
                        stage,
                    },
                )));
            }
        }
        Ok(())
    }

    fn integrate_entity_chunk_load(
        &mut self,
        pos: ChunkPos,
        tag: Option<&CompoundTag>,
        stage: &mut super::WorldgenEntityLoadStage,
    ) -> Result<(), EntityStorageError> {
        let mut loaded = Vec::new();
        let mut remainder = None;
        if let Some(tag) = tag {
            let chunk = EntityChunk::parse(tag.clone())?;
            let position = chunk.position.expect("validated position").0;
            let stored = ChunkPos::new(position[0], position[1]);
            if stored != pos {
                return Err(EntityStorageError::MisplacedChunk {
                    expected: pos,
                    stored,
                });
            }
            let entities = chunk.entities.unwrap_or_default();
            for tag in &entities {
                super::worldgen::check_factories(tag, self)?;
            }
            *stage = super::WorldgenEntityLoadStage::Publishing;
            for tag in entities {
                if super::worldgen::entity_type(&tag).is_none() {
                    eprintln!(
                        "Skipping entity with unknown or missing id while loading entity chunk"
                    );
                    continue;
                }
                match persistence::load_entity_tree(self, tag, &mut loaded) {
                    Ok(_) => (),
                    Err(error @ EntityPersistenceError::Base(_)) => {
                        eprintln!("Skipping invalid entity while loading entity chunk: {error}")
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            remainder = Some(chunk.extra);
        }
        // The stream includes passengers even if their vehicle's duplicate
        // UUID prevents its registration. Keep that Java object reachable
        // solely through held handles, with no index or level callback.
        for (id, state) in loaded {
            if self
                .region()
                .entities
                .registration
                .knows_uuid(state.as_entity().uuid)
            {
                self.retain_unmanaged_entity(id, state);
            } else {
                self.publish(id, state, true, "process_pending_entity_loads_shared")?;
            }
        }
        let mut manager = self.region().entities.persistent.state.borrow_mut();
        if let Some(remainder) = remainder {
            manager.chunk_remainders.insert(pos.pack(), remainder);
        } else {
            manager.empty_chunks.insert(pos.pack());
        }
        manager
            .load_statuses
            .insert(pos.pack(), ChunkLoadStatus::Loaded);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
