use super::*;

impl GameEventListenerRef {
    pub(super) fn behavior(
        self,
        level: &Level,
    ) -> Result<&'static dyn GameEventListener, LevelError> {
        match self {
            Self::BlockEntity(entity) => {
                let state = level.block_entity_state(entity)?;
                state
                    .behavior()
                    .game_event_listener()
                    .ok_or(LevelError::UnsupportedBlockEntity(
                        state.data().block_state.block(),
                    ))
            }
            #[cfg(test)]
            Self::Test(id) => {
                Ok(level.region().game_event_listeners.borrow().tests.listeners[id].behavior)
            }
        }
    }
    fn retain(self, level: &Level) -> Result<(), LevelError> {
        match self {
            Self::BlockEntity(entity) => level.retain_block_entity(entity),
            #[cfg(test)]
            Self::Test(_) => Ok(()),
        }
    }
    pub(super) fn release(self, level: &Level) {
        match self {
            Self::BlockEntity(entity) => level.release_block_entity(entity),
            #[cfg(test)]
            Self::Test(_) => {}
        }
    }
    pub(super) fn deliver(
        self,
        level: &mut Level,
        event: GameEvent,
        context: GameEventContext,
        source: Vec3,
    ) -> Result<bool, LevelError> {
        let behavior = self.behavior(level)?;
        match self {
            Self::BlockEntity(entity) => level.with_block_entity_callback(entity, |level| {
                behavior.handle_game_event(self, level, event, context, source)
            }),
            #[cfg(test)]
            Self::Test(_) => behavior.handle_game_event(self, level, event, context, source),
        }
    }
    pub(super) fn retain_delivery(self, level: &mut Level) -> Result<(), LevelError> {
        self.retain(level)
    }
}

impl Level {
    fn listener_owner(&self, chunk: ChunkPos) -> Result<ChunkListenerOwner, LevelError> {
        let current = self.chunk(chunk).ok_or(LevelError::MissingChunk(chunk))?;
        if let Some(owner) = current.game_event_listener_owner.get() {
            debug_assert_eq!(
                self.region().game_event_listeners.borrow().owners[&owner.0].chunk,
                chunk
            );
            return Ok(owner);
        }
        let mut store = self.region().game_event_listeners.borrow_mut();
        let owner = ChunkListenerOwner(store.next_owner);
        store.next_owner = store
            .next_owner
            .checked_add(1)
            .expect("game-event owner identity exhausted");
        store.owners.insert(
            owner.0,
            ChunkListenerRegistries {
                chunk,
                attached: true,
                posts: 0,
                sections: BTreeMap::new(),
            },
        );
        current.game_event_listener_owner.set(Some(owner));
        Ok(owner)
    }
    pub(super) fn listener_registry(
        &self,
        owner: ChunkListenerOwner,
        section: i32,
    ) -> ListenerRegistryRef {
        let mut store = self.region().game_event_listeners.borrow_mut();
        if let Some(&registry) = store.owners[&owner.0].sections.get(&section) {
            return registry;
        }
        let registry = ListenerRegistryRef(store.next_registry);
        store.next_registry = store
            .next_registry
            .checked_add(1)
            .expect("game-event registry identity exhausted");
        store.registries.insert(
            registry.0,
            GameEventListenerRegistry {
                attached: true,
                ..Default::default()
            },
        );
        store
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .sections
            .insert(section, registry);
        registry
    }
    pub(super) fn collect_listener_registry(&self, registry: ListenerRegistryRef) {
        let state = {
            let mut store = self.region().game_event_listeners.borrow_mut();
            if store
                .registries
                .get(&registry.0)
                .is_none_or(|state| state.attached || state.visits != 0)
            {
                return;
            }
            store.registries.remove(&registry.0).unwrap()
        };
        for listener in state
            .listeners
            .into_iter()
            .chain(state.listeners_to_add)
            .chain(state.listeners_to_remove)
        {
            listener.release(self);
        }
    }
    pub(super) fn release_listener_owner(&self, owner: ChunkListenerOwner) {
        self.region()
            .game_event_listeners
            .borrow_mut()
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .posts -= 1;
        self.collect_listener_owner(owner);
    }
    fn collect_listener_owner(&self, owner: ChunkListenerOwner) {
        let state = {
            let mut store = self.region().game_event_listeners.borrow_mut();
            if store.owners[&owner.0].attached || store.owners[&owner.0].posts != 0 {
                return;
            }
            store.owners.remove(&owner.0).unwrap()
        };
        for registry in state.sections.into_values() {
            self.region()
                .game_event_listeners
                .borrow_mut()
                .registries
                .get_mut(&registry.0)
                .unwrap()
                .attached = false;
            self.collect_listener_registry(registry);
        }
    }
    /// Callback-free metadata registration also serves shared FULL publication.
    /// No cell guard spans listener dispatch or a mutable world callback.
    pub(super) fn register_listener(
        &self,
        listener: GameEventListenerRef,
        pos: BlockPos,
    ) -> Result<(), LevelError> {
        let owner = self.listener_owner(ChunkPos::containing(pos))?;
        let registry = self.listener_registry(owner, pos.y >> 4);
        listener.retain(self)?;
        let mut store = self.region().game_event_listeners.borrow_mut();
        let state = store.registries.get_mut(&registry.0).unwrap();
        if state.processing {
            state.listeners_to_add.push(listener);
        } else {
            state.listeners.push(listener);
            state.modification_count = state.modification_count.wrapping_add(1);
        }
        Ok(())
    }
    pub(super) fn unregister_listener(
        &self,
        listener: GameEventListenerRef,
        pos: BlockPos,
    ) -> Result<(), LevelError> {
        let owner = self.listener_owner(ChunkPos::containing(pos))?;
        let registry = self.listener_registry(owner, pos.y >> 4);
        let (processing, pending) = {
            let store = self.region().game_event_listeners.borrow();
            let state = &store.registries[&registry.0];
            (
                state.processing,
                state.listeners_to_remove.contains(&listener),
            )
        };
        if processing {
            if !pending {
                listener.retain(self)?;
                self.region()
                    .game_event_listeners
                    .borrow_mut()
                    .registries
                    .get_mut(&registry.0)
                    .unwrap()
                    .listeners_to_remove
                    .insert(listener);
            }
        } else {
            let removed = {
                let mut store = self.region().game_event_listeners.borrow_mut();
                let state = store.registries.get_mut(&registry.0).unwrap();
                if let Some(index) = state.listeners.iter().position(|&value| value == listener) {
                    state.listeners.remove(index);
                    state.modification_count = state.modification_count.wrapping_add(1);
                    true
                } else {
                    false
                }
            };
            if removed {
                listener.release(self);
            }
        }
        // Native OnEmptyAction runs here, including during visitation, never
        // after the deferred-removal flush performed by visitInRangeListeners.
        let empty = self.region().game_event_listeners.borrow().registries[&registry.0]
            .listeners
            .is_empty();
        if empty {
            {
                let mut store = self.region().game_event_listeners.borrow_mut();
                store
                    .owners
                    .get_mut(&owner.0)
                    .unwrap()
                    .sections
                    .remove(&(pos.y >> 4));
                store.registries.get_mut(&registry.0).unwrap().attached = false;
            }
            self.collect_listener_registry(registry);
        }
        Ok(())
    }
    pub(super) fn capture_listener_owner(
        &self,
        chunk: ChunkPos,
    ) -> Result<ChunkListenerOwner, LevelError> {
        let owner = self.listener_owner(chunk)?;
        self.region()
            .game_event_listeners
            .borrow_mut()
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .posts += 1;
        Ok(owner)
    }
}

/// Port of LevelChunk#addGameEventListener at the native add/load slot.
pub(crate) fn register_block_entity_listener(
    entity: BlockEntityRef,
    level: &Level,
) -> Result<(), LevelError> {
    if level
        .block_entity_state(entity)?
        .behavior()
        .game_event_listener()
        .is_some()
    {
        level.register_listener(GameEventListenerRef::BlockEntity(entity), entity.pos)?;
    }
    Ok(())
}
/// Port of LevelChunk#removeGameEventListener; direct replacement does not unregister.
pub(crate) fn unregister_block_entity_listener(
    entity: BlockEntityRef,
    level: &Level,
) -> Result<(), LevelError> {
    if level
        .block_entity_state(entity)?
        .behavior()
        .game_event_listener()
        .is_some()
    {
        level.unregister_listener(GameEventListenerRef::BlockEntity(entity), entity.pos)?;
    }
    Ok(())
}
/// Chunk-owned registries detach as a unit. Captured posts/deliveries retain
/// their original registry/listener identities until their stack unwinds.
pub(crate) fn detach_chunk_listeners(chunk: ChunkPos, level: &Level) -> Result<(), LevelError> {
    let owner = level
        .chunk(chunk)
        .ok_or(LevelError::MissingChunk(chunk))?
        .game_event_listener_owner
        .take();
    if let Some(owner) = owner {
        level
            .region()
            .game_event_listeners
            .borrow_mut()
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .attached = false;
        level.collect_listener_owner(owner);
    }
    Ok(())
}
