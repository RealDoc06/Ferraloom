use super::{ChunkPos, HashMapType};
use crate::level::SyncChunk;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

enum UnloadState {
    Live,
    Requested(SyncChunk),
    Prepared(
        Weak<crate::chunk::ChunkData>,
        Option<crate::chunk::PoiCaptureStamp>,
    ),
}

/// Coordinates World-owned block entities with the terrain scheduler. A prepared
/// snapshot belongs to one publication, and any new BE access invalidates it.
#[derive(Default)]
pub struct ChunkUnloadCoordinator {
    state: Mutex<HashMapType<ChunkPos, UnloadState>>,
}

pub struct ChunkUnloadAccess<'a>(MutexGuard<'a, HashMapType<ChunkPos, UnloadState>>);

impl ChunkUnloadCoordinator {
    pub fn access(&self) -> ChunkUnloadAccess<'_> {
        ChunkUnloadAccess(
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl ChunkUnloadAccess<'_> {
    /// Hold this access guard until the returned BE handle has been cloned or
    /// inserted. Final unpublication takes the same guard.
    pub fn mark_live(&mut self, pos: ChunkPos) {
        self.0.insert(pos, UnloadState::Live);
    }

    pub fn published(&mut self, pos: ChunkPos) {
        self.mark_live(pos);
    }

    /// A renewed owner or a publication change invalidates an earlier decision
    /// to retire this position, including snapshots prepared before the renewal.
    pub fn cancel(&mut self, pos: ChunkPos) {
        if self.0.contains_key(&pos) {
            self.mark_live(pos);
        }
    }

    /// Called with ticket/publication locks and the exclusive public map entry.
    /// Every bound terrain publication needs a current POI stamp, even with no
    /// block entities. Unbound standalone fixtures retain the legacy handshake.
    pub fn prepare_or_defer(&mut self, pos: ChunkPos, chunk: &SyncChunk) -> bool {
        match self.0.get(&pos) {
            None if !chunk.has_poi_binding() => return true,
            Some(UnloadState::Prepared(saved, stamp)) if saved.ptr_eq(&Arc::downgrade(chunk)) => {
                if stamp.as_ref().map_or_else(
                    || !chunk.has_poi_binding(),
                    |stamp| chunk.read_for_poi().is_capture_current(stamp),
                ) {
                    return true;
                }
            }
            Some(UnloadState::Requested(requested)) if Arc::ptr_eq(requested, chunk) => {
                return false;
            }
            Some(_) | None => {}
        }
        self.0.insert(pos, UnloadState::Requested(chunk.clone()));
        false
    }

    #[must_use]
    pub fn requests(&self) -> Vec<(ChunkPos, SyncChunk)> {
        self.0
            .iter()
            .filter_map(|(pos, state)| match state {
                UnloadState::Requested(chunk) => Some((*pos, chunk.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn is_requested(&self, pos: ChunkPos, chunk: &SyncChunk) -> bool {
        matches!(self.0.get(&pos), Some(UnloadState::Requested(requested)) if Arc::ptr_eq(requested, chunk))
    }

    pub fn mark_prepared(&mut self, pos: ChunkPos, chunk: &SyncChunk) {
        self.0
            .insert(pos, UnloadState::Prepared(Arc::downgrade(chunk), None));
    }

    /// Call only after POI preparation and the existing BE snapshot succeeded.
    /// Final map removal checks this exact Arc and rechecks its terrain revision.
    pub fn mark_prepared_poi(
        &mut self,
        pos: ChunkPos,
        chunk: &SyncChunk,
        stamp: crate::chunk::PoiCaptureStamp,
    ) {
        self.0.insert(
            pos,
            UnloadState::Prepared(Arc::downgrade(chunk), Some(stamp)),
        );
    }

    pub fn finished(&mut self, pos: ChunkPos) {
        self.0.remove(&pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkData;

    #[test]
    fn prepared_snapshot_cannot_authorize_a_later_publication() {
        let coordinator = ChunkUnloadCoordinator::default();
        let pos = ChunkPos::new(1, -2);
        let original = ChunkData::empty_sync(pos.x, pos.y);
        let replacement = ChunkData::empty_sync(pos.x, pos.y);
        let mut access = coordinator.access();
        access.mark_prepared(pos, &original);
        assert!(!access.prepare_or_defer(pos, &replacement));
        assert!(access.is_requested(pos, &replacement));
        assert!(!access.is_requested(pos, &original));
        access.mark_prepared(pos, &replacement);
        access.published(pos);
        assert!(!access.prepare_or_defer(pos, &replacement));
    }

    #[test]
    fn new_live_handle_invalidates_snapshot_but_plain_level_chunks_need_no_handshake() {
        let coordinator = ChunkUnloadCoordinator::default();
        let pos = ChunkPos::new(0, 0);
        let chunk = ChunkData::empty_sync(pos.x, pos.y);
        let mut access = coordinator.access();
        assert!(access.prepare_or_defer(pos, &chunk));
        access.mark_prepared(pos, &chunk);
        access.mark_live(pos);
        assert!(!access.prepare_or_defer(pos, &chunk));
        access.cancel(pos);
        assert!(!access.is_requested(pos, &chunk));
        assert!(access.requests().is_empty());
        assert!(!access.prepare_or_defer(pos, &chunk));
        access.mark_prepared(pos, &chunk);
        assert!(access.prepare_or_defer(pos, &chunk));
        access.finished(pos);
        assert!(access.requests().is_empty());
    }
}
