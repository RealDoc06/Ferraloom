//! Port of Level#tickBlockEntities and LevelChunk's rebindable ticker wrappers.
//! Lists own wrapper identities; canonical mutable block entities stay in chunks.
use super::{BlockEntityRef, BlockEntityType};
use ferraloom_util::BlockPos;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TickerId(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TickingBlockEntity {
    pub entity: BlockEntityRef,
    pub kind: BlockEntityType,
}
/// Port of RebindableTickingBlockEntityWrapper. None is vanilla's NULL_TICKER.
#[derive(Clone, Debug)]
pub struct RebindableTickingBlockEntityWrapper {
    pub binding: Option<TickingBlockEntity>,
}
#[derive(Debug, Default)]
pub struct BlockEntityTickers {
    wrappers: HashMap<TickerId, RebindableTickingBlockEntityWrapper>,
    at_position: HashMap<BlockPos, TickerId>,
    active: Vec<TickerId>,
    pending: Vec<TickerId>,
    next_id: u64,
    cursor: Option<usize>,
}
impl BlockEntityTickers {
    /// Rebinding preserves the existing list position; removal then insertion
    /// creates a fresh wrapper and therefore waits when called during a phase.
    pub fn bind(&mut self, entity: BlockEntityRef, kind: BlockEntityType) -> TickerId {
        let binding = Some(TickingBlockEntity { entity, kind });
        if let Some(id) = self.at_position.get(&entity.pos).copied() {
            self.wrappers.get_mut(&id).expect("live wrapper").binding = binding;
            return id;
        }
        let id = TickerId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("block entity ticker identities exhausted");
        self.wrappers
            .insert(id, RebindableTickingBlockEntityWrapper { binding });
        self.at_position.insert(entity.pos, id);
        if self.cursor.is_some() {
            self.pending.push(id);
        } else {
            self.active.push(id);
        }
        id
    }
    pub fn unbind(&mut self, pos: BlockPos) {
        if let Some(id) = self.at_position.remove(&pos) {
            self.wrappers.get_mut(&id).expect("live wrapper").binding = None;
        }
    }
    /// Native BoundTickingBlockEntity holds its original receiver until unbound,
    /// including bindings queued or rebound during another callback frame.
    pub(crate) fn bound_entities(&self) -> impl Iterator<Item = BlockEntityRef> + '_ {
        self.wrappers
            .values()
            .filter_map(|wrapper| wrapper.binding.map(|binding| binding.entity))
    }
    pub fn begin_phase(&mut self) {
        assert!(self.cursor.is_none(), "reentrant block entity phase");
        self.active.append(&mut self.pending);
        self.cursor = Some(0);
    }
    /// Short borrow before the callback. Consult each wrapper live, so a previous
    /// callback can remove or rebind a later entry synchronously.
    pub fn next_ticker(&mut self) -> Option<TickingBlockEntity> {
        let mut index = self.cursor.expect("begin_phase first");
        while index < self.active.len() {
            let id = self.active[index];
            if let Some(binding) = self.wrappers[&id].binding {
                index += 1;
                self.cursor = Some(index);
                return Some(binding);
            }
            self.active.remove(index);
            self.wrappers.remove(&id);
        }
        self.cursor = Some(index);
        None
    }
    pub fn end_phase(&mut self) {
        self.cursor = None;
    }
    pub fn is_ticking(&self) -> bool {
        self.cursor.is_some()
    }
    pub fn wrapper(&self, id: TickerId) -> Option<&RebindableTickingBlockEntityWrapper> {
        self.wrappers.get(&id)
    }
}
