//! Synchronous native game-event dispatch with canonical listener receivers.
use super::{Level, LevelError};
use crate::{block::entity::BlockEntityRef, entity::EntityRef};
use ferraloom_data::{BlockState, registries::game_event::Id as GameEvent};
use ferraloom_util::{BlockPos, ChunkPos, Vec3};
use std::collections::{BTreeMap, HashSet};

/// Port of GameEvent.Context. Handles identify the actual source receiver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameEventContext {
    pub source_entity: Option<EntityRef>,
    pub affected_state: Option<BlockState>,
}

/// A listener is owned by its canonical receiver, never by a per-call snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GameEventListenerRef {
    BlockEntity(BlockEntityRef),
    #[cfg(test)]
    Test(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryMode {
    Unspecified,
    ByDistance,
}

/// Port of PositionSource. Entity-backed sources join this enum with their
/// entity-family ports; block sources resolve to the native block center.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PositionSource {
    Block(BlockPos),
}
impl PositionSource {
    pub fn get_position(self, _level: &Level) -> Result<Option<Vec3>, LevelError> {
        Ok(Some(match self {
            Self::Block(pos) => Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
        }))
    }
}

/// Port of GameEventListener. Dispatchers are stateless; every mutable listener
/// field stays on the canonical receiver addressed by the supplied handle.
pub trait GameEventListener: Sync {
    fn get_listener_source(
        &self,
        listener: GameEventListenerRef,
        level: &Level,
    ) -> Result<PositionSource, LevelError>;
    fn get_listener_radius(
        &self,
        listener: GameEventListenerRef,
        level: &Level,
    ) -> Result<i32, LevelError>;
    fn get_delivery_mode(&self) -> DeliveryMode {
        DeliveryMode::Unspecified
    }
    fn handle_game_event(
        &self,
        listener: GameEventListenerRef,
        level: &mut Level,
        event: GameEvent,
        context: GameEventContext,
        source_position: Vec3,
    ) -> Result<bool, LevelError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ChunkListenerOwner(usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ListenerRegistryRef(usize);
struct ChunkListenerRegistries {
    chunk: ChunkPos,
    attached: bool,
    posts: usize,
    sections: BTreeMap<i32, ListenerRegistryRef>,
}
/// Port of EuclideanGameEventListenerRegistry's insertion-ordered listeners and
/// deferred mutations. The removal set is membership-only, never callback order.
#[derive(Default)]
struct GameEventListenerRegistry {
    listeners: Vec<GameEventListenerRef>,
    listeners_to_remove: HashSet<GameEventListenerRef>,
    listeners_to_add: Vec<GameEventListenerRef>,
    processing: bool,
    modification_count: i32,
    attached: bool,
    visits: usize,
}
/// Region-owned storage for chunk-owned registry identities. Detached chunk
/// owners and individual registries remain reachable through in-flight handles.
#[derive(Default)]
pub(super) struct GameEventListenerRegistries {
    next_owner: usize,
    next_registry: usize,
    owners: BTreeMap<usize, ChunkListenerRegistries>,
    registries: BTreeMap<usize, GameEventListenerRegistry>,
    #[cfg(test)]
    tests: test_support::ListenerTests,
}

mod dispatcher;
pub use dispatcher::GameEventDispatcher;
mod registry;
#[expect(
    unused_imports,
    reason = "BE lifecycle owner consumes the register/unregister boundaries after the micro-landing"
)]
pub(crate) use registry::{
    detach_chunk_listeners, register_block_entity_listener, unregister_block_entity_listener,
};
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
