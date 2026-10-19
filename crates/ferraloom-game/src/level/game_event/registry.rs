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
            Self::Test(id) => Ok(level.region().game_event_listeners.tests.listeners[id].behavior),
        }
    }
    fn retain(self, level: &mut Level) -> Result<(), LevelError> {
        match self {
            Self::BlockEntity(entity) => level.retain_block_entity(entity),
            #[cfg(test)]
            Self::Test(_) => Ok(()),
        }
    }
    pub(super) fn release(self, level: &mut Level) {
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
    fn listener_owner(&mut self, chunk: ChunkPos) -> Result<ChunkListenerOwner, LevelError> {
        let current = self
            .chunk(chunk)
            .ok_or(LevelError::MissingChunk(chunk))?
            .game_event_listener_owner;
        if let Some(owner) = current {
            debug_assert_eq!(
                self.region().game_event_listeners.owners[&owner.0].chunk,
                chunk
            );
            return Ok(owner);
        }
        let region = self.region_mut();
        let store = &mut region.game_event_listeners;
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
        region
            .chunks
            .get_mut(&chunk)
            .expect("checked chunk")
            .game_event_listener_owner = Some(owner);
        Ok(owner)
    }
    pub(super) fn listener_registry(
        &mut self,
        owner: ChunkListenerOwner,
        section: i32,
    ) -> ListenerRegistryRef {
        let store = &mut self.region_mut().game_event_listeners;
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
    pub(super) fn collect_listener_registry(&mut self, registry: ListenerRegistryRef) {
        let store = &mut self.region_mut().game_event_listeners;
        if store
            .registries
            .get(&registry.0)
            .is_none_or(|state| state.attached || state.visits != 0)
        {
            return;
        }
        let state = store.registries.remove(&registry.0).unwrap();
        for listener in state
            .listeners
            .into_iter()
            .chain(state.listeners_to_add)
            .chain(state.listeners_to_remove)
        {
            listener.release(self);
        }
    }
    pub(super) fn release_listener_owner(&mut self, owner: ChunkListenerOwner) {
        let store = &mut self.region_mut().game_event_listeners;
        let state = store.owners.get_mut(&owner.0).unwrap();
        state.posts -= 1;
        self.collect_listener_owner(owner);
    }
    fn collect_listener_owner(&mut self, owner: ChunkListenerOwner) {
        let store = &mut self.region_mut().game_event_listeners;
        if store.owners[&owner.0].attached || store.owners[&owner.0].posts != 0 {
            return;
        }
        let state = store.owners.remove(&owner.0).unwrap();
        for registry in state.sections.into_values() {
            self.region_mut()
                .game_event_listeners
                .registries
                .get_mut(&registry.0)
                .unwrap()
                .attached = false;
            self.collect_listener_registry(registry);
        }
    }
    pub(super) fn register_listener(
        &mut self,
        listener: GameEventListenerRef,
        pos: BlockPos,
    ) -> Result<(), LevelError> {
        let owner = self.listener_owner(ChunkPos::containing(pos))?;
        let registry = self.listener_registry(owner, pos.y >> 4);
        listener.retain(self)?;
        let state = self
            .region_mut()
            .game_event_listeners
            .registries
            .get_mut(&registry.0)
            .unwrap();
        if state.processing {
            state.listeners_to_add.push(listener);
        } else {
            state.listeners.push(listener);
            state.modification_count = state.modification_count.wrapping_add(1);
        }
        Ok(())
    }
    pub(super) fn unregister_listener(
        &mut self,
        listener: GameEventListenerRef,
        pos: BlockPos,
    ) -> Result<(), LevelError> {
        let owner = self.listener_owner(ChunkPos::containing(pos))?;
        let registry = self.listener_registry(owner, pos.y >> 4);
        let state = &self.region().game_event_listeners.registries[&registry.0];
        if state.processing {
            if !state.listeners_to_remove.contains(&listener) {
                listener.retain(self)?;
                self.region_mut()
                    .game_event_listeners
                    .registries
                    .get_mut(&registry.0)
                    .unwrap()
                    .listeners_to_remove
                    .insert(listener);
            }
        } else {
            let state = self
                .region_mut()
                .game_event_listeners
                .registries
                .get_mut(&registry.0)
                .unwrap();
            if let Some(index) = state.listeners.iter().position(|&value| value == listener) {
                state.listeners.remove(index);
                state.modification_count = state.modification_count.wrapping_add(1);
                listener.release(self);
            }
        }
        // Native unregister invokes OnEmptyAction here, even during visitation;
        // visitInRangeListeners does not invoke it after its deferred removals.
        if self.region().game_event_listeners.registries[&registry.0]
            .listeners
            .is_empty()
        {
            self.region_mut()
                .game_event_listeners
                .owners
                .get_mut(&owner.0)
                .unwrap()
                .sections
                .remove(&(pos.y >> 4));
            self.region_mut()
                .game_event_listeners
                .registries
                .get_mut(&registry.0)
                .unwrap()
                .attached = false;
            self.collect_listener_registry(registry);
        }
        Ok(())
    }
    pub(super) fn capture_listener_owner(
        &mut self,
        chunk: ChunkPos,
    ) -> Result<ChunkListenerOwner, LevelError> {
        let owner = self.listener_owner(chunk)?;
        self.region_mut()
            .game_event_listeners
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .posts += 1;
        Ok(owner)
    }
}

/// Port of LevelChunk#addGameEventListener. The lifecycle owner calls this at
/// the native add/load slot, with the original block-entity generation.
#[expect(
    dead_code,
    reason = "BE lifecycle owner wires this boundary after the dispatcher micro-landing"
)]
pub(crate) fn register_block_entity_listener(
    entity: BlockEntityRef,
    level: &mut Level,
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
/// Port of LevelChunk#removeGameEventListener. Direct setBlockEntity replacement
/// does not call this helper; removal retains its original receiver.
#[expect(
    dead_code,
    reason = "BE lifecycle owner wires this boundary after the dispatcher micro-landing"
)]
pub(crate) fn unregister_block_entity_listener(
    entity: BlockEntityRef,
    level: &mut Level,
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
/// Detach the chunk's registry ownership, without unregistering every listener.
/// Active posts continue against the captured owner; queued deliveries hold
/// their own listener roots until the complete post has returned.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "BE lifecycle owner wires this boundary after the dispatcher micro-landing"
    )
)]
pub(crate) fn detach_chunk_listeners(chunk: ChunkPos, level: &mut Level) -> Result<(), LevelError> {
    let owner = level
        .region_mut()
        .chunks
        .get_mut(&chunk)
        .ok_or(LevelError::MissingChunk(chunk))?
        .game_event_listener_owner
        .take();
    if let Some(owner) = owner {
        level
            .region_mut()
            .game_event_listeners
            .owners
            .get_mut(&owner.0)
            .unwrap()
            .attached = false;
        level.collect_listener_owner(owner);
    }
    Ok(())
}
